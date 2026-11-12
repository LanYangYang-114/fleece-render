# syntax=docker/dockerfile:1.7
FROM rust:1.90-bookworm@sha256:3914072ca0c3b8aad871db9169a651ccfce30cf58303e5d6f2db16d1d8a7e58f AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --locked --bin fleece-render

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /var/lib/fleece-render fleece-render
COPY --from=builder /app/target/release/fleece-render /usr/local/bin/fleece-render
COPY fleece-render.toml /etc/fleece-render.toml
COPY LICENSE THIRD_PARTY_NOTICES.md \
    crates/fleece-render-core/assets/LICENSE-minecraft-font-CC-BY-SA-3.0.txt \
    /usr/share/doc/fleece-render/
ENV FLEECE_RENDER_CONFIG=/etc/fleece-render.toml \
    FLEECE_RENDER_HOST=0.0.0.0 \
    FLEECE_RENDER_LOG_FILE=""
WORKDIR /var/lib/fleece-render
EXPOSE 57680
USER fleece-render
ENTRYPOINT ["/usr/local/bin/fleece-render"]
