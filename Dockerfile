# syntax=docker/dockerfile:1.7
# shape-rs — shape-solver API image (workspace: crates/shape + bins).
# Built ONLY in CI (GitHub Actions, linux/amd64) and published to GHCR.

ARG RUST_VERSION=1.94

FROM rust:${RUST_VERSION}-bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY vendor ./vendor
RUN --mount=type=cache,target=/src/target \
    cargo build --release --bin shapeserver \
 && cp target/release/shapeserver /usr/local/bin/shapeserver

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/shapeserver /usr/local/bin/
ENV SHAPE_HOST=0.0.0.0
EXPOSE 9150
CMD ["shapeserver"]
