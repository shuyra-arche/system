use super::execution::{
    begin, build, checked_run, effective_harts, evidence, record_hashes, root, run_qemu,
    selected_harts,
};
use crate::options::Common;
use kernel_verification::{Result, TARGET, capsule, json, require};
use std::{fs, path::Path, time::Duration};

pub(crate) const RUNTIME_MARKERS: [&str; 4] = [
    "PASS system-runtime heap=1 growing=1 shared=1 owner=1 worker-exit=1 reclaim=1 contention=1",
    "PASS supply-native rights=1 scope=1 replay=1 cancel=1 reorder=1 withdrawal=1 cleanup=1",
    "PASS supply-window relative=1 narrowed=1 once=1 cleanup=1",
    "PASS system-rpc abi=13 protocol=4 context=1 sender=1 reply=1 cleanup=1 worker-reclaimed=1",
];

pub(crate) fn run(args: Common) -> Result<()> {
    require(
        args.profile == "release",
        "runtime fixture preserves its release-only build",
    )?;
    let (out, kernel, kernel_sha) = begin(&args)?;
    let harts = effective_harts(&args.harts, &[1, 4]);
    let root = root();
    let target = out.join("build");
    build(
        &root.join("server/tests/target/runtime/Cargo.toml"),
        TARGET,
        &["--release"],
        &[],
        &target,
        &[],
        &out.join("build-runtime"),
    )?;
    let elf = target.join(TARGET).join("release/system-runtime-guest");
    // System scenarios use a dense Capsule so every BSS page is materialized.
    let image = out.join("runtime.initrd");
    fs::write(&image, capsule::system_initrd(&fs::read(&elf)?)?)?;
    let harts = selected_harts(&harts, &[1, 4])?;
    let mut inputs = evidence(&args, &out, &kernel, &kernel_sha, "runtime", &harts)?;
    record_hashes(
        &mut inputs,
        &[
            kernel.clone(),
            elf.clone(),
            image.clone(),
            root.join("server/tests/target/runtime/Cargo.lock"),
            root.join("server/tests/target/runtime/platform/Cargo.lock"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"),
        ],
    )?;
    json(&out.join("inputs.json"), &inputs)?;
    for hart in harts {
        let dir = out.join(format!("harts-{hart}"));
        let outcome = run_qemu(&kernel, &image, hart, &dir, Duration::from_secs(120), false)?;
        checked_run(
            outcome,
            &dir,
            &RUNTIME_MARKERS,
            &["FAIL", "[kernel] fatal", "page fault killed"],
        )?;
    }
    Ok(())
}
