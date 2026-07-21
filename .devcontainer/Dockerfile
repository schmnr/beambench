FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Install Node.js LTS
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs

# Install Tauri v2 system dependencies (WebKitGTK for Linux)
RUN apt-get update && apt-get install -y \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    libssl-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    && rm -rf /var/lib/apt/lists/*

# Install X11 client libs for display forwarding
RUN apt-get update && apt-get install -y \
    x11-apps \
    libx11-dev \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-nextest
RUN cargo install cargo-nextest --locked

# Install Tauri CLI
RUN cargo install tauri-cli --version "^2" --locked
