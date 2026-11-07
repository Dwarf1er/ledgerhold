# syntax=docker/dockerfile:1
#
# Self-hosted Ledgerhold: the web UI plus the server, in one small image.
#   docker build -t ledgerhold .
#   docker run -p 3000:3000 -v ledgerhold-data:/data ledgerhold
#   docker logs <container>     # the one-time setup code; enter it in the browser to choose a password
# See docker-compose.yml for the usual way to run it.

# ---- 1. Build the web UI --------------------------------------------------------------
FROM node:lts-alpine AS ui
WORKDIR /src/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

# ---- 2. Build the server --------------------------------------------------------------
FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
# The desktop shell (Tauri) isn't needed here and drags in GTK/WebKit: drop it from the workspace.
# (That prunes Cargo.lock, so `--locked` can't be used; every remaining crate keeps its locked version.)
RUN sed -i 's/, "src-tauri"//' Cargo.toml
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p ledgerhold-server \
 && install -d -m 700 /out/data \
 && cp target/release/ledgerhold-server /out/

# ---- 3. Runtime: no shell, no package manager, not root --------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=server /out/ledgerhold-server /usr/local/bin/ledgerhold-server
# A pre-owned /data means a fresh named volume is writable by the non-root user.
COPY --from=server --chown=65532:65532 --chmod=0700 /out/data /data
COPY --from=ui /src/ui/dist /app/ui

ENV LEDGERHOLD_LISTEN=0.0.0.0:3000 \
    LEDGERHOLD_DB=/data/ledgerhold.sqlite \
    LEDGERHOLD_STATIC=/app/ui \
    TZ=UTC

VOLUME /data
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/ledgerhold-server", "--healthcheck"]

USER nonroot
ENTRYPOINT ["/usr/local/bin/ledgerhold-server"]
