#!/usr/bin/env sh

set -e
set -x

rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu
rustup target add x86_64-unknown-linux-gnu --toolchain nightly

RUSTFLAGS="-Zlocation-detail=none -Zfmt-debug=none" \
	cargo +nightly build --release --locked \
	-Z build-std=std,panic_abort -Z build-std-features= \
	--target x86_64-unknown-linux-gnu
