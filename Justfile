run:
    cargo build --workspace --target-dir target
    ZENZAI_BIN_DIR="{{ justfile_directory() }}/target/debug" cargo run --target-dir target -p xdg-desktop-portal-zenzai
