# Multi-stage build for Rust TinyPNG
FROM rust:1.75-bullseye as builder

# Install system dependencies needed for compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libpng-dev \
    libjpeg-turbo8-dev \
    libwebp-dev \
    libtiff-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Copy source code and assets
COPY src ./src
COPY assets ./assets

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libpng16-16 \
    libjpeg-turbo8 \
    libwebp6 \
    libtiff5 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/rust_tinypng_clone /app/rust_tinypng_clone

# Create a non-root user
RUN useradd -m -u 1001 appuser && chown -R appuser:appuser /app
USER appuser

# Expose port
EXPOSE 3030

# Set environment variables
ENV RUST_LOG=info
ENV PORT=3030

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3030/ || exit 1

# Run the application
CMD ["./rust_tinypng_clone"]