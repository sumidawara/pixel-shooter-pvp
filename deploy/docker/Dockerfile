# syntax=docker/dockerfile:1

FROM rust:1.92-bookworm AS builder
ARG BINARY
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY backend backend
COPY tools/debug-web/dist tools/debug-web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin "${BINARY}" \
    && cp "/app/target/release/${BINARY}" /app/service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home pixel-shooter
COPY --from=builder /app/service /usr/local/bin/pixel-shooter-service
USER pixel-shooter
ENTRYPOINT ["/usr/local/bin/pixel-shooter-service"]
