# syntax=docker/dockerfile:1

FROM scratch AS server-source
ADD https://github.com/fhfelipefh/pontemesh-server.git#5ea3ab9a3c5986052466bcb0fe76e0a72a747a17 /source

FROM node:22-bookworm-slim AS web-build
WORKDIR /app/web
COPY --from=server-source /source/web/package.json /source/web/package-lock.json ./
RUN npm ci
COPY --from=server-source /source/web ./
RUN npm exec vite build

FROM rust:1-bookworm AS rust-build
WORKDIR /app
COPY --from=server-source /source/Cargo.toml /source/Cargo.lock ./
COPY --from=server-source /source/src ./src
COPY --from=server-source /source/migrations ./migrations
COPY --from=web-build /app/web/dist ./web/dist
RUN cargo build --release

FROM debian:bookworm-slim
ENV PONTEMESH_HOME=/var/pontemesh_home
ENV RUST_LOG=pontemesh_server=info,tower_http=info
RUN useradd --system --create-home --home-dir /nonexistent --shell /usr/sbin/nologin pontemesh \
    && mkdir -p /var/pontemesh_home \
    && chown -R pontemesh:pontemesh /var/pontemesh_home
WORKDIR /app
COPY --from=rust-build /app/target/release/pontemesh-server /usr/local/bin/pontemesh-server
COPY --from=rust-build /app/migrations ./migrations
USER pontemesh
EXPOSE 8080 9000
VOLUME ["/var/pontemesh_home"]
ENTRYPOINT ["/usr/local/bin/pontemesh-server"]
