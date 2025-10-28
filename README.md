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
3. Choose quality (Good/Better/Best) and output format
4. Click "🚀 Convert Images" button
5. Download compressed results

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

**Parameters**:
- `file`: Image file to compress
- `output_format`: `original`, `png`, `jpeg`, `webp`, `avif`, `tiff`, `bmp`, `ico`
- `png_quality`: Quality range (e.g., `50-80`)
- `oxipng`: Boolean (`true`/`false`)
- `png_lossy`: Boolean (`true`/`false`)

**Example with cURL**:
```bash
curl -X POST http://localhost:3030/api/compress \
  -F "file=@image.png" \
  -F "output_format=webp" \
  -F "png_quality=60-85" \
  -F "oxipng=true" \
  -F "png_lossy=true" \
  -o compressed_image.webp
```

**Example with Postman**:
1. Set method to `POST`
2. URL: `http://localhost:3030/api/compress`
3. Body → form-data:
   - `file`: Select image file
   - `output_format`: `webp` (or desired format)
   - `png_quality`: `50-80`
   - `oxipng`: `true`
   - `png_lossy`: `true`

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
