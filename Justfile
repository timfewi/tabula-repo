# Optional shortcuts; Cargo remains the primary development entrypoint.
check:
    cargo fmt --all -- --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked

package:
    cargo package --locked --allow-dirty

release:
    cargo build --locked --release
