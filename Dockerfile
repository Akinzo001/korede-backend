# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.96
ARG SUI_VERSION=1.72.1
ARG SUI_RELEASE_TAG=testnet-v1.72.1
ARG SUI_ARCHIVE=sui-testnet-v1.72.1-ubuntu-x86_64.tgz

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src

RUN cargo build --release --bin korede_backend_server

FROM ubuntu:24.04 AS sui-cli

ARG SUI_VERSION
ARG SUI_RELEASE_TAG
ARG SUI_ARCHIVE

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tar \
    && rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,id=sui-cli-download,target=/var/cache/sui-download \
    set -eux; \
    mkdir -p /opt/sui; \
    SUI_DOWNLOAD="/var/cache/sui-download/${SUI_ARCHIVE}"; \
    curl --fail --location --show-error \
        --retry 10 \
        --retry-all-errors \
        --retry-delay 5 \
        --retry-max-time 1800 \
        --continue-at - \
        --output "${SUI_DOWNLOAD}" \
        "https://github.com/MystenLabs/sui/releases/download/${SUI_RELEASE_TAG}/${SUI_ARCHIVE}"; \
    tar -xzf "${SUI_DOWNLOAD}" -C /opt/sui; \
    SUI_BIN="$(find /opt/sui -type f -name sui -perm /111 | head -n 1)"; \
    test -n "$SUI_BIN"; \
    install -m 0755 "$SUI_BIN" /usr/local/bin/sui; \
    rm -rf /opt/sui; \
    sui --version; \
    sui --version | grep "${SUI_VERSION}"

FROM ubuntu:24.04 AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libpq5 libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system app \
    && useradd --system --create-home --gid app app \
    && { getent group 1000 >/dev/null || groupadd --gid 1000 render-secrets; } \
    && usermod --append --groups "$(getent group 1000 | cut -d: -f1)" app \
    && mkdir -p /app/storage /tmp/korede-sui \
    && chown -R app:app /app /tmp/korede-sui

WORKDIR /app

COPY --from=builder /app/target/release/korede_backend_server /usr/local/bin/korede_backend_server
COPY --from=sui-cli /usr/local/bin/sui /usr/local/bin/sui
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh

RUN chmod 0755 /usr/local/bin/korede_backend_server \
    /usr/local/bin/sui \
    /usr/local/bin/docker-entrypoint.sh

ENV APP_HOST=0.0.0.0
ENV SUI_CLI_PATH=/usr/local/bin/sui
ENV SUI_KEYSTORE_PATH=/etc/secrets/sui.keystore
ENV SUI_CLIENT_CONFIG_PATH=/tmp/korede-sui/client.yaml

EXPOSE 10000

USER app

ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["korede_backend_server"]
