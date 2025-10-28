![UI_2](https://github.com/bmurrtech/rust-tinypng-clone/blob/main/assets/UI_2.webp)
![UI_1](https://github.com/bmurrtech/rust-tinypng-clone/blob/main/assets/UI_1.webp)
# 🖼️ Rust TinyPNG

A fast, private image compressor with TinyPNG-like behavior, built entirely in Rust. Features both a beautiful web UI and powerful CLI for local image compression with no external dependencies.

## ✨ Features

- **🚀 Fast**: Native Rust performance with parallel processing
- **🔒 Private**: 100% local processing - no uploads to external servers
- **🎨 Multiple Formats**: PNG, JPEG, WebP, AVIF, TIFF, BMP, ICO, HEIC support
- **💡 Smart Compression**: TinyPNG-like PNG quantization + oxipng optimization
- **🌐 Web UI**: Modern, responsive interface with quality slider and format buttons
- **⚡ CLI Mode**: Command-line interface for batch processing
- **📦 Portable**: Single binary with embedded web assets
- **🐳 Docker Ready**: Containerized deployment available

## 🚀 Quick Start

### Option 1: Download Release Binary
1. Download the latest release from [GitHub Releases](https://github.com/bmurrtech/rust-tinypng-clone/releases)
2. Run the binary:
   ```bash
   ./rust_tinypng_clone
   ```
3. Open your browser to `http://localhost:3030`

### Option 2: Build from Source
1. Install Rust from [rustup.rs](https://rustup.rs)
2. Clone and build:
   ```bash
   git clone https://github.com/bmurrtech/rust-tinypng-clone.git
   cd rust-tinypng-clone
   cargo build --release
   ./target/release/rust_tinypng_clone
   ```

### Option 3: Docker
```bash
# Build and run with Docker
docker build -t rust-tinypng .
docker run -p 3030:3030 rust-tinypng
```

## 💻 Usage

### Web Interface
1. Launch the application (it auto-opens your browser)
2. Select images using the "📁 Select Images" button
3. Choose compression level (Low/Mid/Max) and output format
4. Click the Compress button
5. Download compressed results (button changes to "Download All" after compression)

### CLI Mode
```bash
# Compress images in a directory
./rust_tinypng_clone /path/to/images --output ./compressed

# Convert to WebP
./rust_tinypng_clone /path/to/images --to-webp

# Convert to AVIF with custom quality
./rust_tinypng_clone /path/to/images --to-avif --png-quality 40-70

# Overwrite originals
./rust_tinypng_clone /path/to/images --overwrite
```

## 🛠 API Documentation

### Compression Endpoint
```bash
POST http://localhost:3030/api/compress
```

**Content-Type**: `multipart/form-data`

**Parameters** (one of `file` or `media_url` required):
- `file` *(required)*: Local image file path (e.g., `/path/to/file/image.png` or `C:\path\to\file\image.png`)
- `media_url` *(required)*: Public URL to image (e.g., `https://example.com/image.png`, S3, GCP bucket, Azure Blob, etc.)
- `compression_lvl` *(optional)*: Compression preset (`low`, `mid`, `max`)
  - `low`: Best quality (70-90 range)
  - `mid`: Balanced (50-80 range) - *default*
  - `max`: Smallest file (20-60 range)
- `output_format` *(optional)*: `original`, `png`, `jpeg`, `webp`, `avif`, `tiff`, `bmp`, `ico` (default: `webp`)
- `oxipng` *(optional)*: Boolean (`true`/`false`, default: `true`)
- `png_lossy` *(optional)*: Boolean (`true`/`false`, default: `true`)

**Example with cURL**:
```bash
# Minimal request with local file (uses defaults: mid compression, webp output)
curl -X POST http://localhost:3030/api/compress \
  -F "file=@/path/to/file/image.png" \
  -o compressed_image.webp

# With compression level preset
curl -X POST http://localhost:3030/api/compress \
  -F "file=@image.png" \
  -F "compression_lvl=max" \
  -o compressed_image.webp

# Compress from remote URL (S3, GCP, Azure Blob, etc.)
curl -X POST http://localhost:3030/api/compress \
  -F "media_url=https://example.s3.amazonaws.com/image.png" \
  -F "compression_lvl=mid" \
  -F "output_format=webp" \
  -o compressed_image.webp

# Custom output format and compression
curl -X POST http://localhost:3030/api/compress \
  -F "file=@image.png" \
  -F "compression_lvl=low" \
  -F "output_format=png" \
  -F "oxipng=true" \
  -F "png_lossy=true" \
  -o compressed_image.png
```

**Example with Postman**:
1. Set method to `POST`
2. URL: `http://localhost:3030/api/compress`
3. Body → form-data (use either `file` OR `media_url`):
   - `file`: Select local image file or enter path like `/path/to/file/image.png` *(required if not using media_url)*
   - `media_url`: Enter public URL like `https://example.com/image.png` *(required if not using file)*
   - `compression_lvl`: `mid` (or `low`, `max`) - *default: mid*
   - `output_format`: `webp` (optional, defaults to webp)
   - `oxipng`: `true` (optional, default: true)
   - `png_lossy`: `true` (optional, default: true)

## 🧪 Testing

Run the test suite:
```bash
cargo test
```

Test the API:
```bash
# Start the server
./rust_tinypng_clone &

# Test PNG compression
curl -X POST http://localhost:3030/api/compress \
  -F "file=@test_image.png" \
  -F "output_format=original" \
  -o compressed.png
```

## 🔧 Configuration

Copy `.env.example` to `.env` and customize:
```bash
cp .env.example .env
```

Available settings:
- `PORT`: Server port (default: 3030)
- `RUST_LOG`: Log level (info, debug, warn, error)
- `MAX_FILE_SIZE_MB`: Maximum file size limit
- `DEFAULT_PNG_QUALITY`: Default quality range

## 🧬 Supported Formats

| Input | Output | Notes |
|-------|-----------|-------|
| PNG | PNG, WebP, AVIF, JPEG, TIFF, BMP, ICO | TinyPNG-like quantization |
| JPEG | JPEG, WebP, AVIF, PNG, TIFF, BMP, ICO | mozjpeg optimization |
| HEIC/HEIF | JPEG | Auto-converts like TinyPNG |
| WebP | All formats | Full decode/re-encode |
| TIFF, BMP | All formats | Standard image processing |

## 📄 License

**CC BY-NC 4.0** - Creative Commons Attribution-NonCommercial 4.0 International

- ✅ **Share & Adapt**: Copy, redistribute, remix, and build upon the material
- ✅ **Attribution Required**: Give appropriate credit and indicate changes
- ❌ **No Commercial Use**: Cannot be used for commercial purposes

For commercial licensing, please contact the author.

## 🤝 Contributing

Contributions welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality  
4. Submit a pull request

## ⚡ Performance Notes

- **PNG**: Uses libimagequant for TinyPNG-like compression + oxipng optimization
- **JPEG**: mozjpeg encoder with progressive mode and trellis quantization
- **WebP**: High-quality lossy encoding optimized for web
- **AVIF**: Modern format with superior compression ratios
- **Parallel Processing**: Automatic CPU detection for optimal performance
