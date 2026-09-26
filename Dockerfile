# syntax=docker/dockerfile:1.7
# Dockerfile
#
# Debian + Node 22 + Rust stable + wasm32. Bind-mount the repo at /app.
# Named volumes hold node_modules, pnpm store, and the cargo registry.

# Docker Hub, as in drawnosaurus. public.ecr.aws meters anonymous pulls per source IP, and
# a shared network runs out of it ("429 toomanyrequests: Data limit exceeded").
FROM node:22-bookworm-slim

RUN apt-get update \
	&& apt-get install -y --no-install-recommends ca-certificates curl git build-essential pkg-config \
	&& rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:${PATH}
ENV DRAW_ENGINE_IN_DOCKER=1

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal \
	&& rustup target add wasm32-unknown-unknown \
	&& rustup component add rustfmt clippy

# Prebuilt wasm-bindgen-cli (compiling it from crates.io is minutes of LLVM).
ARG WASM_BINDGEN_VERSION=0.2.128
RUN curl -sSL "https://github.com/rustwasm/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
	| tar -xz -C /usr/local/bin --strip-components=1 \
	&& chmod +x /usr/local/bin/wasm-bindgen /usr/local/bin/wasm2es6js || true

ENV PNPM_HOME=/pnpm
ENV PATH=${PNPM_HOME}:${PATH}

RUN corepack enable \
	&& corepack prepare pnpm@10.32.1 --activate \
	&& mkdir -p /pnpm/store \
	&& pnpm config set store-dir /pnpm/store

WORKDIR /app

COPY scripts/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["bash", "scripts/docker-run.sh", "test"]
