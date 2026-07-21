#!/bin/bash
set -e

echo "=== Beam Bench Dev Container Setup ==="

# Ensure Rust is up to date
rustup update stable
rustup component add clippy rustfmt

# Install frontend dependencies (if package.json exists)
if [ -f "tauri-app/package.json" ]; then
  echo "Installing npm dependencies..."
  cd tauri-app && npm ci && cd ..
fi

# Fetch Rust dependencies (if Cargo.toml exists)
if [ -f "Cargo.toml" ]; then
  echo "Fetching Cargo dependencies..."
  cargo fetch
fi

echo "=== Setup complete ==="
