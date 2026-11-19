fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("riscv64") {
        return;
    }
    let script = std::env::var("DEP_SHUYRA_KERNEL_LINKER_SCRIPT").expect("Kernel linker contract");
    println!(
        "cargo::rustc-link-arg-bin=system-runtime-platform=--defsym=KERNEL_LOAD_ADDRESS=0x80200000"
    );
    println!("cargo::rustc-link-arg-bin=system-runtime-platform=-T{script}");
    println!("cargo::rerun-if-changed={script}");
}
