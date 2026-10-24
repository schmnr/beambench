FROM ubuntu:22.04
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl build-essential binutils dbus gnome-keyring \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --profile minimal --default-toolchain 1.96.0 \
    && rm /tmp/rustup.sh
ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /probe
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked \
    && readelf -d target/release/beambench-credential-store-probe \
    && ! readelf -d target/release/beambench-credential-store-probe \
       | grep -Ei 'NEEDED.*(libsecret|libdbus|libssl|libcrypto|libglib|libgio|libgobject|libsystemd)'
CMD ["/probe/target/release/beambench-credential-store-probe", "--mock"]
