ARG BASE_IMAGE
FROM ${BASE_IMAGE}
RUN apk add --no-cache bash alpine-sdk util-linux file autoconf automake libtool xz \
    eudev-dev gettext-dev linux-headers meson zstd-dev zstd-static zlib-dev \
    zlib-static clang musl-dev mimalloc-dev
COPY source /runtime
COPY inputs /inputs
COPY build-runtime.sh /build-runtime.sh
RUN bash /build-runtime.sh
