.PHONY: build-sbf

build-sbf:
	cargo build-sbf --manifest-path programs/prop-amm/Cargo.toml
