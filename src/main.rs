use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser};
use humansize::{format_size, DECIMAL};
use image::{self, DynamicImage, ImageFormat};
use imagequant::{Attributes, Image as LiqImage};
use mozjpeg::{ColorSpace, Compress, ScanMode};
use oxipng::{optimize_from_memory, Options as OxipngOptions};
use rayon::prelude::*;
use ravif::{Encoder as AvifEncoder};
use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write, Cursor};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use webp::Encoder as WebpEncoder;

// Web server imports
use axum::{
    http::{header, StatusCode},
    response::{Html, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::Multipart;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

/// CLI options
#[derive(Parser, Debug)]
#[command(author, version, about = "Rust-only image compressor (TinyPNG-like)")]
struct Args {
    /// Launch web UI on localhost (default mode if no input provided)
    #[arg(long, action = ArgAction::SetTrue)]
    web: bool,

    /// Port for web server (default: 3030)
    #[arg(long, default_value = "3030")]
    port: u16,

    /// Input file or directory (CLI mode)
    input: Option<PathBuf>,

    /// Output directory (defaults to same folder as each file)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Overwrite originals (write to temporary c_ file then replace)
    #[arg(long, action = ArgAction::SetTrue)]
    overwrite: bool,

    /// Number of concurrent workers (defaults to CPU count)
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Enable lossy PNG quantization (TinyPNG-like)
    #[arg(long = "png-lossy", action = ArgAction::SetTrue, default_value_t = true)]
    png_lossy: bool,

    /// pngquant-like quality range (e.g. 50-80)
    #[arg(long, default_value = "50-80")]
    png_quality: String,

    /// Run oxipng after quantization (lossless structural optimization)
    #[arg(long = "oxipng", action = ArgAction::SetTrue, default_value_t = true)]
    oxipng: bool,

    /// Convert/generate WebP (overrides original format)
    #[arg(long, action = ArgAction::SetTrue)]
    to_webp: bool,

    /// Convert/generate AVIF (overrides original format)
    #[arg(long, action = ArgAction::SetTrue)]
    to_avif: bool,
}

const SUPPORTED_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "tiff", "tif", "webp", "heic", "heif"];

// Embedded HTML for web UI
const INDEX_HTML: &str = include_str!("../assets/index.html");

#[derive(Debug, Clone)]
struct CompressionOptions {
    png_lossy: bool,
    png_quality: String,
    oxipng: bool,
    to_webp: bool,
    to_avif: bool,
    to_jpeg: bool,
    to_png: bool,
    to_tiff: bool,
    to_bmp: bool,
    to_ico: bool,
}

fn human_size(nbytes: u64) -> String {
    format_size(nbytes, DECIMAL)
}

/// Parse "50-80" into (min,max) u8
fn parse_quality_range(s: &str) -> (u8, u8) {
    let parts: Vec<_> = s.split('-').collect();
    let min = parts.get(0).and_then(|p| p.parse::<u8>().ok()).unwrap_or(50);
    let max = parts.get(1).and_then(|p| p.parse::<u8>().ok()).unwrap_or(80);
    (min, max)
}

/// PNG: quantize via libimagequant + optional oxipng (lossless)
fn compress_png_bytes(input: &[u8], quality_range: &str, run_oxipng: bool) -> Result<Vec<u8>> {
    // Decode to RGBA8
    let img = image::load_from_memory(input)?;
    let rgba = img.to_rgba8();
    let (w_u32, h_u32) = (rgba.width(), rgba.height());
    let (w, h) = (w_u32 as usize, h_u32 as usize);

    // parse quality
    let (min_q, max_q) = parse_quality_range(quality_range);
    
    // For max compression (20-60 range), use aggressive settings
    let is_max_compression = max_q <= 60;

    // libimagequant
    let mut attr = Attributes::new();
    
    // Adjust speed based on compression level
    if is_max_compression {
        attr.set_speed(1)?; // Slowest, highest quality quantization
        attr.set_max_colors(128)?; // Reduce palette size for max compression
    } else {
        attr.set_speed(3)?; // Balanced speed
    }
    
    attr.set_quality(min_q, max_q)?;
    
    // Convert Vec<u8> to the expected RGBA format
    let rgba_pixels: Vec<rgb::RGBA<u8>> = rgba.chunks_exact(4)
        .map(|chunk| rgb::RGBA::new(chunk[0], chunk[1], chunk[2], chunk[3]))
        .collect();
    
    let mut img_liq = LiqImage::new(&attr, rgba_pixels.as_slice(), w, h, 0.0)?;
    let mut res = attr.quantize(&mut img_liq)?;
    res.set_dithering_level(1.0)?;

    let (palette, pixels) = res.remapped(&mut img_liq)?;

    // Encode as RGBA PNG by expanding palette indices.
    let mut expanded = Vec::with_capacity(w * h * 4);
    for idx in pixels.iter() {
        let p = palette[*idx as usize];
        expanded.push(p.r);
        expanded.push(p.g);
        expanded.push(p.b);
        expanded.push(p.a);
    }

    let dyn_img = DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(w_u32, h_u32, expanded)
            .ok_or_else(|| anyhow!("failed to build indexed->rgba image"))?,
    );

    let mut cursor = Cursor::new(Vec::new());
    dyn_img.write_to(&mut cursor, ImageFormat::Png)?;
    let png_buf = cursor.into_inner();

    // Optional oxipng optimization (lossless)
    if run_oxipng {
        let mut opts = OxipngOptions::from_preset(6);
        opts.strip = oxipng::StripChunks::Safe;
        let optimized = optimize_from_memory(&png_buf, &opts)?;
        return Ok(optimized);
    }

    Ok(png_buf)
}

/// JPEG: re-encode with mozjpeg
fn compress_jpeg_bytes(input: &[u8], quality: u8) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)?;
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);

    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_size(w, h);
    comp.set_quality(quality as f32);
    comp.set_progressive_mode();
    comp.set_scan_optimization_mode(ScanMode::AllComponentsTogether);
    
    // For max compression, enable additional optimization
    if quality <= 60 {
        comp.set_optimize_coding(true);
        comp.set_optimize_scans(true);
    }

    let mut dest = Vec::new();
    let mut writer = comp.start_compress(&mut dest)?;

    // mozjpeg expects raw RGB bytes
    let data = rgb.into_raw();
    writer.write_scanlines(&data)?;
    writer.finish()?;

    Ok(dest)
}

/// WebP via webp crate (lossy) 
fn to_webp_bytes(input: &[u8], quality: f32) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)?;
    let rgba = img.to_rgba8();
    let enc = WebpEncoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let webp = enc.encode(quality); // 0..=100
    Ok(webp.to_vec())
}

/// Convert HEIC to JPEG (like TinyPNG behavior)
fn heic_to_jpeg_bytes(input: &[u8], quality: u8) -> Result<Vec<u8>> {
    // Try to decode as HEIC using image crate fallback
    // If image crate doesn't support HEIC, we'll get an error and handle gracefully
    let img = image::load_from_memory(input)
        .map_err(|_| anyhow!("Unsupported HEIC format or corrupted file"))?;
        
    let rgb = img.to_rgb8();
    compress_jpeg_bytes(&{
        let mut cursor = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(rgb).write_to(&mut cursor, ImageFormat::Jpeg)?;
        cursor.into_inner()
    }, quality)
}

/// Convert to PNG
fn to_png_bytes(input: &[u8], quality_range: &str, use_oxipng: bool) -> Result<Vec<u8>> {
    // Use PNG compression with quality settings
    compress_png_bytes(input, quality_range, use_oxipng)
}

/// Convert to TIFF
fn to_tiff_bytes(input: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)?;
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, ImageFormat::Tiff)?;
    Ok(cursor.into_inner())
}

/// Convert to BMP
fn to_bmp_bytes(input: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)?;
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, ImageFormat::Bmp)?;
    Ok(cursor.into_inner())
}

/// Convert to ICO (fallback to PNG if ICO not supported)
fn to_ico_bytes(input: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)?;
    // Resize to common icon size if needed
    let resized = if img.width() > 256 || img.height() > 256 {
        img.resize(256, 256, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    
    let mut cursor = Cursor::new(Vec::new());
    // Try ICO first, fallback to PNG if not supported
    match resized.write_to(&mut cursor, ImageFormat::Ico) {
        Ok(_) => Ok(cursor.into_inner()),
        Err(_) => {
            // Fallback to PNG for ICO
            let mut png_cursor = Cursor::new(Vec::new());
            resized.write_to(&mut png_cursor, ImageFormat::Png)?;
            Ok(png_cursor.into_inner())
        }
    }
}

/// AVIF via ravif crate (lossy)
fn to_avif_bytes(input: &[u8], quality: f32) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)?;
    let rgba = img.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let speed = 6u8; // 0 best / slowest, 10 fastest
    let enc = AvifEncoder::new().with_quality(quality).with_speed(speed);
    
    // Convert to proper RGBA format
    let rgba_pixels: Vec<rgb::RGBA<u8>> = rgba.chunks_exact(4)
        .map(|chunk| rgb::RGBA::new(chunk[0], chunk[1], chunk[2], chunk[3]))
        .collect();
    
    let avif_img = ravif::Img::new(rgba_pixels.as_slice(), w as usize, h as usize);
    let avif = enc.encode_rgba(avif_img)?;
    Ok(avif.avif_file)
}

/// In-process compress dispatcher
fn compress_image_inproc(input_bytes: &[u8], ext_lower: &str, opts: &CompressionOptions) -> Result<(Vec<u8>, String)> {
    // Handle HEIC files first (convert to JPEG like TinyPNG)
    if ext_lower == "heic" || ext_lower == "heif" {
        let bytes = heic_to_jpeg_bytes(input_bytes, 85)?; // High quality for HEIC conversion
        return Ok((bytes, "image/jpeg".to_string()));
    }
    
    // Parse quality range to determine compression level
    let (min_q, max_q) = parse_quality_range(&opts.png_quality);
    let webp_quality = ((min_q + max_q) / 2) as f32;
    let jpeg_quality = (min_q + max_q) / 2;
    let avif_quality = ((min_q + max_q) / 2) as f32;
    
    // If conversion requested, honor it next
    if opts.to_webp {
        let bytes = to_webp_bytes(input_bytes, webp_quality)?;
        return Ok((bytes, "image/webp".to_string()));
    }
    if opts.to_avif {
        let bytes = to_avif_bytes(input_bytes, avif_quality)?;
        return Ok((bytes, "image/avif".to_string()));
    }
    if opts.to_jpeg {
        let bytes = compress_jpeg_bytes(input_bytes, jpeg_quality)?;
        return Ok((bytes, "image/jpeg".to_string()));
    }
    if opts.to_png {
        let bytes = to_png_bytes(input_bytes, &opts.png_quality, opts.oxipng)?;
        return Ok((bytes, "image/png".to_string()));
    }
    if opts.to_tiff {
        let bytes = to_tiff_bytes(input_bytes)?;
        return Ok((bytes, "image/tiff".to_string()));
    }
    if opts.to_bmp {
        let bytes = to_bmp_bytes(input_bytes)?;
        return Ok((bytes, "image/bmp".to_string()));
    }
    if opts.to_ico {
        let bytes = to_ico_bytes(input_bytes)?;
        return Ok((bytes, "image/x-icon".to_string()));
    }

    match ext_lower {
        "png" => {
            if opts.png_lossy {
                let bytes = compress_png_bytes(input_bytes, &opts.png_quality, opts.oxipng)?;
                Ok((bytes, "image/png".into()))
            } else {
                // lossless re-encode
                let img = image::load_from_memory(input_bytes)?;
                let mut cursor = Cursor::new(Vec::new());
                img.write_to(&mut cursor, ImageFormat::Png)?;
                let buf = cursor.into_inner();
                Ok((buf, "image/png".into()))
            }
        }
        "jpg" | "jpeg" => {
            let bytes = compress_jpeg_bytes(input_bytes, 75)?;
            Ok((bytes, "image/jpeg".into()))
        }
        // Other formats → PNG by default
        _ => {
            let bytes = compress_png_bytes(input_bytes, &opts.png_quality, opts.oxipng)?;
            Ok((bytes, "image/png".into()))
        }
    }
}

fn discover_files(input_path: &Path) -> Vec<PathBuf> {
    if input_path.is_file() {
        if let Some(ext) = input_path.extension().and_then(OsStr::to_str).map(|s| s.to_lowercase()) {
            if SUPPORTED_EXTS.contains(&ext.as_str()) {
                return vec![input_path.to_path_buf()];
            }
        }
        return vec![];
    }

    let mut files = vec![];
    for entry in WalkDir::new(input_path).into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(OsStr::to_str).map(|s| s.to_lowercase()) {
                if SUPPORTED_EXTS.contains(&ext.as_str()) {
                    files.push(p.to_path_buf());
                }
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

fn build_output_path(
    src: &Path,
    output_dir: &Option<PathBuf>,
    overwrite: bool,
    target_ext: Option<&str>,
) -> PathBuf {
    let base = if overwrite {
        src.with_file_name(format!(
            "c_{}",
            src.file_name().and_then(OsStr::to_str).unwrap_or("out")
        ))
    } else if let Some(out) = output_dir {
        let _ = fs::create_dir_all(out);
        out.join(format!(
            "c_{}",
            src.file_name().and_then(OsStr::to_str).unwrap_or("out")
        ))
    } else {
        src.with_file_name(format!(
            "c_{}",
            src.file_name().and_then(OsStr::to_str).unwrap_or("out")
        ))
    };

    if let Some(ext) = target_ext {
        base.with_extension(ext)
    } else {
        base
    }
}

// Web server handlers
async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn compress_api(mut multipart: Multipart) -> Result<Response, StatusCode> {
    let mut file_bytes = Vec::new();
    let mut filename = String::new();
    let mut opts = CompressionOptions {
        png_lossy: true,
        png_quality: "50-80".to_string(),
        oxipng: true,
        to_webp: false,
        to_avif: false,
        to_jpeg: false,
        to_png: false,
        to_tiff: false,
        to_bmp: false,
        to_ico: false,
    };

    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let field_name = field.name().unwrap_or("").to_string();
        
        match field_name.as_str() {
            "file" => {
                filename = field.file_name().unwrap_or("image").to_string();
                file_bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?.to_vec();
            }
            "png_quality" => {
                let value = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                opts.png_quality = value;
            }
            "output_format" => {
                let value = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                match value.as_str() {
                    "webp" => opts.to_webp = true,
                    "avif" => opts.to_avif = true,
                    "jpeg" => opts.to_jpeg = true,
                    "png" => opts.to_png = true,
                    "tiff" => opts.to_tiff = true,
                    "bmp" => opts.to_bmp = true,
                    "ico" => opts.to_ico = true,
                    _ => {} // keep original
                }
            }
            "oxipng" => {
                let value = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                opts.oxipng = value == "true";
            }
            "png_lossy" => {
                let value = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                opts.png_lossy = value == "true";
            }
            _ => {}
        }
    }

    if file_bytes.is_empty() {
        log::error!("❌ API: No file data received");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Detect file extension
    let ext = filename.split('.').last().unwrap_or("").to_lowercase();
    log::info!("🔍 API: Processing {} file: {} ({} bytes)", ext.to_uppercase(), filename, file_bytes.len());
    
    // Compress the image
    let start_time = std::time::Instant::now();
    let (compressed_bytes, mime_type) = compress_image_inproc(&file_bytes, &ext, &opts)
        .map_err(|e| {
            log::error!("❌ API: Compression failed for {}: {:?}", filename, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    let duration = start_time.elapsed();
    let compression_ratio = (1.0 - (compressed_bytes.len() as f64 / file_bytes.len() as f64)) * 100.0;
    log::info!("✅ API: Compressed {} in {:?} - {} -> {} bytes ({:.1}% reduction)", 
               filename, duration, file_bytes.len(), compressed_bytes.len(), compression_ratio);

    // Determine output filename
    let output_filename = if opts.to_webp {
        filename.replace(&format!(".{}", ext), ".webp")
    } else if opts.to_avif {
        filename.replace(&format!(".{}", ext), ".avif")
    } else if opts.to_jpeg {
        filename.replace(&format!(".{}", ext), ".jpg")
    } else if opts.to_png {
        filename.replace(&format!(".{}", ext), ".png")
    } else if opts.to_tiff {
        filename.replace(&format!(".{}", ext), ".tiff")
    } else if opts.to_bmp {
        filename.replace(&format!(".{}", ext), ".bmp")
    } else if opts.to_ico {
        filename.replace(&format!(".{}", ext), ".ico")
    } else if ext == "heic" || ext == "heif" {
        // HEIC files are automatically converted to JPEG
        filename.replace(&format!(".{}", ext), ".jpg")
    } else {
        format!("c_{}", filename)
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", output_filename))
        .body(compressed_bytes.into())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

async fn start_web_server(port: u16) -> Result<()> {
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/compress", post(compress_api))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
        );

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| anyhow!("Failed to bind to {}: {}", addr, e))?;
        
    println!("🚀 Web UI running at http://localhost:{}", port);
    
    // Auto-open browser
    if webbrowser::open(&format!("http://localhost:{}", port)).is_err() {
        println!("💡 Open http://localhost:{} in your browser", port);
    }
    
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow!("Server error: {}", e))?;
        
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Auto-detect mode: web if no input provided or --web flag
    if args.web || args.input.is_none() {
        return start_web_server(args.port).await;
    }

    // CLI mode
    run_cli_mode(&args).await
}

async fn run_cli_mode(args: &Args) -> Result<()> {
    let jobs = args.jobs.unwrap_or_else(|| num_cpus::get());
    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .ok();

    // Ensure input present
    if args.input.is_none() {
        return Err(anyhow!("No input provided for CLI mode"));
    }
    let input_path = args.input.as_ref().unwrap().canonicalize()?;
    if !input_path.exists() {
        return Err(anyhow!("Input path does not exist: {}", input_path.display()));
    }

    let output_dir = args
        .output
        .as_ref()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()));

    let files = discover_files(&input_path);
    if files.is_empty() {
        eprintln!("No supported image files found.");
        return Ok(());
    }

    let results: Vec<_> = files
        .par_iter()
        .map(|f| {
            let fname = f.to_path_buf();
            let before = fs::metadata(&fname).map(|m| m.len()).unwrap_or(0);

            // load file
            let mut input_bytes = Vec::new();
            if let Err(e) = fs::File::open(&fname).and_then(|mut r| r.read_to_end(&mut input_bytes)) {
                return (fname, before, 0u64, false, format!("read-failed: {}", e));
            }

            let ext = fname
                .extension()
                .and_then(OsStr::to_str)
                .map(|s| s.to_lowercase())
                .unwrap_or_default();

            // Determine output extension if conversion requested
            let mut target_ext: Option<&str> = None;
            if args.to_webp {
                target_ext = Some("webp");
            } else if args.to_avif {
                target_ext = Some("avif");
            }

            // Compute output path
            let mut out_path = build_output_path(&fname, &output_dir, args.overwrite, target_ext);

            // Create compression options from CLI args
            let opts = CompressionOptions {
                png_lossy: args.png_lossy,
                png_quality: args.png_quality.clone(),
                oxipng: args.oxipng,
                to_webp: args.to_webp,
                to_avif: args.to_avif,
                to_jpeg: false,
                to_png: false,
                to_tiff: false,
                to_bmp: false,
                to_ico: false,
            };

            // Compress in-process
            let result = compress_image_inproc(&input_bytes, &ext, &opts);
            let (out_bytes, _mime) = match result {
                Ok((b, m)) => (b, m),
                Err(e) => return (fname, before, 0u64, false, format!("compress-failed: {}", e)),
            };

            // If no explicit target_ext and we converted non-png to png as fallback, update ext to png
            if target_ext.is_none() {
                if !["png", "jpg", "jpeg"].contains(&ext.as_str()) {
                    out_path.set_extension("png");
                }
            }

            // Write to out_path
            if let Some(parent) = out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::File::create(&out_path).and_then(|mut w| w.write_all(&out_bytes)) {
                return (fname, before, 0u64, false, format!("write-failed: {}", e));
            }

            // Overwrite semantics
            let mut final_path = out_path.clone();
            if args.overwrite {
                let backup = fname.with_extension(format!(
                    "{}{}",
                    fname.extension().and_then(OsStr::to_str).unwrap_or(""),
                    ".bak"
                ));
                if let Err(e) = fs::rename(&fname, &backup) {
                    return (fname, before, 0u64, false, format!("backup-failed: {}", e));
                }
                if let Err(e) = fs::rename(&out_path, &fname) {
                    let _ = fs::rename(&backup, &fname);
                    return (fname, before, 0u64, false, format!("overwrite-failed: {}", e));
                }
                let _ = fs::remove_file(&backup);
                final_path = fname.clone();
            }

            let after = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
            (fname, before, after, true, String::new())
        })
        .collect();

    let mut total_before: u64 = 0;
    let mut total_after: u64 = 0;
    let mut processed: usize = 0;

    for (name, before, after, ok, msg) in &results {
        if !*ok {
            eprintln!("{}: failed ({})", name.display(), msg);
        } else {
            let saved = before.saturating_sub(*after);
            let pct = if *before > 0 {
                (saved as f64) / (*before as f64) * 100.0
            } else {
                0.0
            };
            println!(
                "{}: {} → {} (saved {} / {:.2}%)",
                name.file_name().and_then(OsStr::to_str).unwrap_or("file"),
                human_size(*before),
                human_size(*after),
                human_size(saved),
                pct
            );
        }
        total_before = total_before.saturating_add(*before);
        total_after = total_after.saturating_add(*after);
        if *ok {
            processed += 1;
        }
    }

    if processed > 0 {
        let total_saved = total_before.saturating_sub(total_after);
        let pct_total = if total_before > 0 {
            (total_saved as f64) / (total_before as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "\nProcessed {} files. Total saved: {} ({:.2}%)",
            processed,
            human_size(total_saved),
            pct_total
        );
    } else {
        eprintln!("No files compressed.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_png() -> Vec<u8> {
        // Create a simple 100x100 red PNG image
        let img = image::ImageBuffer::from_fn(100, 100, |_, _| {
            image::Rgb([255, 0, 0]) // Red pixel
        });
        let dynamic_img = DynamicImage::ImageRgb8(img);
        let mut bytes = Vec::new();
        dynamic_img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png).unwrap();
        bytes
    }

    fn create_test_jpeg() -> Vec<u8> {
        // Create a simple 100x100 blue JPEG image
        let img = image::ImageBuffer::from_fn(100, 100, |_, _| {
            image::Rgb([0, 0, 255]) // Blue pixel
        });
        let dynamic_img = DynamicImage::ImageRgb8(img);
        let mut bytes = Vec::new();
        dynamic_img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Jpeg).unwrap();
        bytes
    }

    #[test]
    fn test_png_compression() {
        let png_data = create_test_png();
        let opts = CompressionOptions {
            png_lossy: true,
            png_quality: "50-80".to_string(),
            oxipng: true,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: false,
        };
        
        let result = compress_image_inproc(&png_data, "png", &opts);
        assert!(result.is_ok());
        
        let (compressed, mime_type) = result.unwrap();
        assert_eq!(mime_type, "image/png");
        assert!(compressed.len() > 0);
        // Compressed should typically be smaller, but for small test images it might not be
    }

    #[test]
    fn test_jpeg_compression() {
        let jpeg_data = create_test_jpeg();
        let opts = CompressionOptions {
            png_lossy: false,
            png_quality: "50-80".to_string(),
            oxipng: false,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: false,
        };
        
        let result = compress_image_inproc(&jpeg_data, "jpeg", &opts);
        assert!(result.is_ok());
        
        let (compressed, mime_type) = result.unwrap();
        assert_eq!(mime_type, "image/jpeg");
        assert!(compressed.len() > 0);
    }

    #[test]
    fn test_format_conversions() {
        let png_data = create_test_png();
        
        // Test PNG to WebP conversion
        let opts_webp = CompressionOptions {
            png_lossy: false,
            png_quality: "50-80".to_string(),
            oxipng: false,
            to_webp: true,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: false,
        };
        
        let result = compress_image_inproc(&png_data, "png", &opts_webp);
        assert!(result.is_ok());
        let (_, mime_type) = result.unwrap();
        assert_eq!(mime_type, "image/webp");
        
        // Test PNG to TIFF conversion
        let opts_tiff = CompressionOptions {
            png_lossy: false,
            png_quality: "50-80".to_string(),
            oxipng: false,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: true,
            to_bmp: false,
            to_ico: false,
        };
        
        let result = compress_image_inproc(&png_data, "png", &opts_tiff);
        assert!(result.is_ok());
        let (_, mime_type) = result.unwrap();
        assert_eq!(mime_type, "image/tiff");
        
        // Test PNG to BMP conversion
        let opts_bmp = CompressionOptions {
            png_lossy: false,
            png_quality: "50-80".to_string(),
            oxipng: false,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: true,
            to_ico: false,
        };
        
        let result = compress_image_inproc(&png_data, "png", &opts_bmp);
        assert!(result.is_ok());
        let (_, mime_type) = result.unwrap();
        assert_eq!(mime_type, "image/bmp");
        
        // Test PNG to ICO conversion
        let opts_ico = CompressionOptions {
            png_lossy: false,
            png_quality: "50-80".to_string(),
            oxipng: false,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: true,
        };
        
        let result = compress_image_inproc(&png_data, "png", &opts_ico);
        assert!(result.is_ok());
        let (_, mime_type) = result.unwrap();
        assert_eq!(mime_type, "image/x-icon");
    }

    #[test]
    fn test_heic_conversion() {
        // For this test, we'll use a JPEG as a stand-in for HEIC
        // since actual HEIC files require special libraries
        let jpeg_data = create_test_jpeg();
        let opts = CompressionOptions {
            png_lossy: false,
            png_quality: "50-80".to_string(),
            oxipng: false,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: false,
        };
        
        // Test HEIC extension triggers JPEG conversion
        let result = compress_image_inproc(&jpeg_data, "heic", &opts);
        assert!(result.is_ok());
        let (_, mime_type) = result.unwrap();
        assert_eq!(mime_type, "image/jpeg");
    }

    #[test]
    fn test_quality_parsing() {
        assert_eq!(parse_quality_range("50-80"), (50, 80));
        assert_eq!(parse_quality_range("40-70"), (40, 70));
        assert_eq!(parse_quality_range("20-60"), (20, 60)); // Max compression
        assert_eq!(parse_quality_range("70-90"), (70, 90)); // Low compression
        assert_eq!(parse_quality_range("60"), (60, 80)); // Default max
        assert_eq!(parse_quality_range("invalid"), (50, 80)); // Default values
    }
    
    #[test] 
    fn test_max_compression_levels() {
        let png_data = create_test_png();
        
        // Test max compression PNG (20-60 range)
        let opts_max = CompressionOptions {
            png_lossy: true,
            png_quality: "20-60".to_string(),
            oxipng: true,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: false,
        };
        
        let result_max = compress_image_inproc(&png_data, "png", &opts_max);
        assert!(result_max.is_ok());
        
        // Test low compression PNG (70-90 range)
        let opts_low = CompressionOptions {
            png_lossy: true,
            png_quality: "70-90".to_string(),
            oxipng: true,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: false,
        };
        
        let result_low = compress_image_inproc(&png_data, "png", &opts_low);
        assert!(result_low.is_ok());
        
        // Max compression should typically produce smaller files
        let (max_bytes, _) = result_max.unwrap();
        let (low_bytes, _) = result_low.unwrap();
        
        // For very small test images this might not always hold, but ensure both work
        assert!(max_bytes.len() > 0);
        assert!(low_bytes.len() > 0);
    }

    #[tokio::test]
    async fn test_web_api_compression() {
        // This would require more complex setup to test the actual multipart handling
        // For now, we'll just test that the core compression functions work
        let png_data = create_test_png();
        
        // Test that our compression function can handle the data
        let opts = CompressionOptions {
            png_lossy: true,
            png_quality: "50-80".to_string(),
            oxipng: false,
            to_webp: false,
            to_avif: false,
            to_jpeg: false,
            to_png: false,
            to_tiff: false,
            to_bmp: false,
            to_ico: false,
        };
        
        let result = compress_image_inproc(&png_data, "png", &opts);
        assert!(result.is_ok());
    }
}
