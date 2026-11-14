fn main() {
    println!(
        "cargo:rustc-link-arg-bin=system=-T{}/link.ld",
        env!("CARGO_MANIFEST_DIR")
    );
    println!("cargo:rerun-if-changed=link.ld");
}
