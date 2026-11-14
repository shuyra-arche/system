fn main() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../server/link.ld")
        .canonicalize()
        .unwrap();
    println!("cargo:rustc-link-arg-bin=system=-T{}", script.display());
    println!("cargo:rerun-if-changed={}", script.display());
}
