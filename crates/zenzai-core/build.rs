fn main() {
    // Shared resources live outside each crate, so Cargo must watch them explicitly.
    println!("cargo:rerun-if-changed=../../locales");
}
