fn main() {
    let script = std::env::var("DEP_EXECUTION_LINKER_SCRIPT")
        .expect("execution did not publish its linker script");
    println!("cargo::rustc-link-arg-bin=system=-T{script}");
    println!("cargo::rerun-if-changed={script}");
}
