FROM rust:1.92-slim-bookworm

WORKDIR /app

# Install deps in single layer
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl \
    && rm -rf /var/lib/apt/lists/* \
    && curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall -y cargo-watch

# Cache cargo registry
ENV CARGO_HOME=/usr/local/cargo
VOLUME ["/usr/local/cargo/registry", "/usr/local/cargo/git"]

EXPOSE 8000

CMD ["cargo", "watch", "--poll", "-x", "run --bin fissio-server"]
