#!/bin/bash
# Cross-platform build script for Rust TinyPNG

set -e

echo "🚀 Building Rust TinyPNG releases..."

# Clean previous builds
cargo clean

# Create releases directory
mkdir -p releases

# Build for macOS (current platform)
echo "📦 Building for macOS (x86_64-apple-darwin)..."
cargo build --release
cp target/release/rust_tinypng_clone releases/rust_tinypng_clone-macos-x64

# Build for macOS Apple Silicon (if available)
if rustup target list --installed | grep -q "aarch64-apple-darwin"; then
    echo "📦 Building for macOS Apple Silicon (aarch64-apple-darwin)..."
    cargo build --release --target aarch64-apple-darwin
    cp target/aarch64-apple-darwin/release/rust_tinypng_clone releases/rust_tinypng_clone-macos-arm64
fi

# Add Windows target if not already installed
if ! rustup target list --installed | grep -q "x86_64-pc-windows-gnu"; then
    echo "🔧 Adding Windows target..."
    rustup target add x86_64-pc-windows-gnu
fi

# Install mingw-w64 if not available (macOS)
if [[ "$OSTYPE" == "darwin"* ]]; then
    if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
        echo "🔧 Installing mingw-w64..."
        brew install mingw-w64
    fi
fi

# Build for Windows
echo "📦 Building for Windows (x86_64-pc-windows-gnu)..."
cargo build --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/rust_tinypng_clone.exe releases/rust_tinypng_clone-windows-x64.exe

# Add Linux target if not already installed
if ! rustup target list --installed | grep -q "x86_64-unknown-linux-gnu"; then
    echo "🔧 Adding Linux target..."
    rustup target add x86_64-unknown-linux-gnu
fi

# Try to build for Linux (may require additional setup)
echo "📦 Attempting to build for Linux (x86_64-unknown-linux-gnu)..."
if cargo build --release --target x86_64-unknown-linux-gnu 2>/dev/null; then
    cp target/x86_64-unknown-linux-gnu/release/rust_tinypng_clone releases/rust_tinypng_clone-linux-x64
    echo "✅ Linux build successful"
else
    echo "⚠️  Linux cross-compilation skipped (requires additional setup)"
fi

# Create checksums
echo "🔐 Creating checksums..."
cd releases
for file in rust_tinypng_clone-*; do
    if [[ -f "$file" ]]; then
        shasum -a 256 "$file" > "$file.sha256"
    fi
done
cd ..

# List created files
echo "📋 Created releases:"
ls -la releases/

echo "✅ Build complete! Releases are in the ./releases directory"
echo ""
echo "📝 Platform Notes:"
echo "   🍎 macOS: May need 'chmod +x' and 'xattr -d com.apple.quarantine'"
echo "   🧾 Windows: Unsigned .exe may trigger SmartScreen warnings"
echo "   🐧 Linux: Built for glibc-based distributions (Ubuntu, Debian, etc.)"
echo "   📦 Debian ARM64: For Raspberry Pi 4+, AWS Graviton, Apple Silicon Linux"
