FROM debian:bookworm-slim AS build-bookworm
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential debhelper dpkg-dev ca-certificates curl git \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o rustup.sh \
    && chmod +x ./rustup.sh \
    && ./rustup.sh -y
WORKDIR /build/source
COPY ./rsgit.service ./Cargo.toml ./Cargo.lock .
COPY ./src ./src
COPY ./debian ./debian
RUN \
    ls -alh && \
    apt-get update && \
    env PATH="$PATH:$HOME/.cargo/bin" \
        DEB_BUILD_OPTIONS="noautodbgsym nocheck" \
        dpkg-buildpackage -us -uc -b && \
    rm -rf /var/lib/apt/lists/* && \
    install -d /out && \
    cp ../*.deb ../*.buildinfo ../*.changes /out/ 2>/dev/null; \
    ls /out/*.deb >/dev/null

FROM debian:trixie-slim AS build-trixie
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential debhelper dpkg-dev ca-certificates curl git \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o rustup.sh \
    && chmod +x ./rustup.sh \
    && ./rustup.sh -y
WORKDIR /build/source
COPY ./rsgit.service ./Cargo.toml ./Cargo.lock .
COPY ./src ./src
COPY ./debian ./debian
RUN \
    ls -alh && \
    apt-get update && \
    env PATH="$PATH:$HOME/.cargo/bin" \
        DEB_BUILD_OPTIONS="noautodbgsym nocheck" \
        dpkg-buildpackage -us -uc -b && \
    rm -rf /var/lib/apt/lists/* && \
    install -d /out && \
    cp ../*.deb ../*.buildinfo ../*.changes /out/ 2>/dev/null; \
    ls /out/*.deb >/dev/null

FROM scratch AS export
COPY --from=build-bookworm /out/ /debian12/
COPY --from=build-trixie   /out/ /debian13/
