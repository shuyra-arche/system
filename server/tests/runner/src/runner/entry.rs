use super::execution::{
    begin, build, checked_run, effective_harts, evidence, record_hashes, root, run_command,
    run_qemu, selected_harts,
};
use crate::options::Entry;
use kernel_verification::{Result, TARGET, capsule, json, require};
use std::{fs, path::Path, process::Command, time::Duration};

const ENTRY_MARKERS: [&str; 3] = [
    "PASS system-entry words=4 ledger=13 catalog=2 dtb=1 heap=1",
    "PASS entry-services heap=production roles=2 root=1 wired=1 retire=1",
    "PASS boot-data native=1 readonly=1 content=1 padding=1 retire=1",
];

pub(crate) fn run(args: Entry) -> Result<()> {
    let common = args.common;
    require(
        common.profile == "release",
        "entry fixture preserves its release-only build",
    )?;
    let (out, kernel, kernel_sha) = begin(&common)?;
    let harts = effective_harts(&common.harts, &[1, 4]);
    let mode = if args.abort {
        "abort"
    } else if args.stall {
        "stall"
    } else {
        "normal"
    };
    let root = root();
    let target = out.join("build");
    let features = if args.abort {
        vec!["--features", "abort"]
    } else if args.stall {
        vec!["--features", "stall"]
    } else {
        vec![]
    };
    build(
        &root.join("server/tests/target/entry/Cargo.toml"),
        TARGET,
        &["--release"],
        &features,
        &target,
        &[],
        &out.join("build-entry"),
    )?;
    let elf = target.join(TARGET).join("release/system");
    let packed_capsule = capsule::system_initrd(&fs::read(&elf)?)?;
    let boot = out.join("capsule");
    fs::write(&boot, &packed_capsule[4096..])?;
    let archive = out.join("initrd");
    pack_entry(&root, &boot, &elf, &archive, &out.join("pack-entry"))?;
    let harts = selected_harts(&harts, &[1, 4])?;
    let mut inputs = evidence(&common, &out, &kernel, &kernel_sha, mode, &harts)?;
    record_hashes(
        &mut inputs,
        &[
            kernel.clone(),
            elf.clone(),
            archive.clone(),
            root.join("server/tests/target/entry/Cargo.lock"),
            root.join("server/tests/target/entry/pack/Cargo.lock"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"),
        ],
    )?;
    json(&out.join("inputs.json"), &inputs)?;
    for hart in harts {
        let dir = out.join(format!("harts-{hart}"));
        let outcome = run_qemu(
            &kernel,
            &archive,
            hart,
            &dir,
            Duration::from_secs(90),
            false,
        )?;
        let markers: Vec<&str> = if args.abort {
            vec![
                "EXPECTED entry-abort workers=2 backing=live",
                "exit tid=1 reason=0x2a note: system: injected entry failure",
                "all tasks exited",
            ]
        } else if args.stall {
            vec![
                "EXPECTED entry-stall timed-out=1",
                "exit tid=1 reason=0x2b note: system: stalled entry bootstrap",
                "all tasks exited",
            ]
        } else {
            vec![
                ENTRY_MARKERS[0],
                ENTRY_MARKERS[1],
                ENTRY_MARKERS[2],
                "exit tid=1 reason=0x0 note: system: complete",
                "all tasks exited",
            ]
        };
        checked_run(
            outcome,
            &dir,
            &markers,
            &["panicked", "page fault killed", "reason=4294967295"],
        )?;
    }
    Ok(())
}

fn pack_entry(root: &Path, boot: &Path, system: &Path, archive: &Path, log: &Path) -> Result<()> {
    let mut command = Command::new("cargo");
    command
        .args([
            "run",
            "--locked",
            "--release",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--manifest-path",
        ])
        .arg(root.join("server/tests/target/entry/pack/Cargo.toml"))
        .args(["--"])
        .arg(boot)
        .arg(system)
        .arg(archive)
        .current_dir(root);
    let mut result = run_command(&mut command, log, Duration::from_secs(900))?;
    result.passed = result.success();
    json(&log.join("result.json"), &result)?;
    require(
        result.success(),
        format!("entry pack failed; evidence: {}", log.display()),
    )
}
