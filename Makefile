.PHONY: build-sbf test

build-sbf:
	cargo build-sbf --manifest-path programs/prop-amm/Cargo.toml
	cargo build-sbf --manifest-path c_u_soon/program/Cargo.toml

test: build-sbf
	cargo test -p prop-amm
