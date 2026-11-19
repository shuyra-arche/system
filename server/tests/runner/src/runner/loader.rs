use super::execution::{
    begin, build, checked_run, effective_harts, evidence, profile, record_hashes, root, run_qemu,
    selected_harts,
};
use crate::options::Loader;
use kernel_verification::{Result, TARGET, capsule, digest, git, json, output, require, revision};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const SYSTEM_MARKER: &str = "PASS loader-elf native=1 user=1 arguments=1 private=1 cache=1 eviction=1 cancel=1 running-stop=1 journal=1 reclaim=1";

pub(crate) fn run(args: Loader) -> Result<()> {
    let common = args.common;
    let (out, kernel, kernel_sha) = begin(&common)?;
    let harts = effective_harts(&common.harts, &[1, 2, 4]);
    let root = root();
    let target = out.join("build");
    let image_manifest = root.join("server/tests/target/loader/image/Cargo.toml");
    let guest_manifest = root.join("server/tests/target/loader/Cargo.toml");
    let image_elf = target.join(TARGET).join("release/system-loader-image");
    build(
        &image_manifest,
        TARGET,
        &["--release"],
        &[],
        &target,
        &[],
        &out.join("build-image"),
    )?;
    let mut envs = vec![(
        "SYSTEM_LOADER_IMAGE".to_owned(),
        image_elf.display().to_string(),
    )];
    let mut marker = SYSTEM_MARKER;
    let mut input_json = evidence(&common, &out, &kernel, &kernel_sha, "loader", &harts)?;
    if let Some(receipt_path) = args.toolchain_receipt {
        let receipt_path = receipt_path.canonicalize()?;
        let receipt: Value = serde_json::from_slice(&fs::read(&receipt_path)?)?;
        require(
            receipt["target_build"] == "passed",
            "Toolchain target build receipt is not passed",
        )?;
        let artifact = &receipt["artifacts"][&args.toolchain_fixture];
        let toolchain_elf = PathBuf::from(
            artifact["path"]
                .as_str()
                .ok_or("missing Toolchain ELF path")?,
        );
        let toolchain_elf = toolchain_elf.canonicalize()?;
        require(
            digest(&toolchain_elf)?
                == artifact["elf"]["sha256"]
                    .as_str()
                    .ok_or("missing Toolchain ELF hash")?,
            "Toolchain ELF does not match its receipt",
        )?;
        let toolchain_root = receipt_path
            .parent()
            .and_then(Path::parent)
            .ok_or("invalid Toolchain receipt location")?;
        let revision = revision(toolchain_root)?;
        let dirty = !output(git(toolchain_root).args(["status", "--porcelain"]))?.is_empty();
        require(
            !dirty,
            "commit Toolchain source inputs before composition verification",
        )?;
        envs.push((
            "SYSTEM_TOOLCHAIN_IMAGE".to_owned(),
            toolchain_elf.display().to_string(),
        ));
        marker = "PASS loader-toolchain user=1 std=1 threads=1 reclaim=1 retirement=1";
        input_json["toolchain"] = json!({
            "revision": revision,
            "dirty": dirty,
            "fixture": args.toolchain_fixture,
            "receipt": receipt,
            "receipt_sha256": digest(&receipt_path)?,
            "elf": toolchain_elf,
            "toolchain_sha256": digest(&toolchain_elf)?,
            "elf_sha256": digest(&toolchain_elf)?
        });
    }
    let feature = if marker == SYSTEM_MARKER {
        vec![]
    } else {
        vec!["--features", "toolchain"]
    };
    build(
        &guest_manifest,
        TARGET,
        &profile(&common.profile),
        &feature,
        &target,
        &envs,
        &out.join("build-loader"),
    )?;
    let guest_name = if common.profile == "release" {
        "release"
    } else {
        "debug"
    };
    let guest_elf = target
        .join(TARGET)
        .join(guest_name)
        .join("system-loader-guest");
    let image = out.join("loader.initrd");
    fs::write(&image, capsule::system_initrd(&fs::read(&guest_elf)?)?)?;
    fs::copy(
        root.join("server/tests/target/loader/Cargo.lock"),
        out.join("guest.Cargo.lock"),
    )?;
    fs::copy(
        root.join("server/tests/target/loader/image/Cargo.lock"),
        out.join("image.Cargo.lock"),
    )?;
    fs::copy(
        root.join("server/tests/target/runtime/platform/Cargo.lock"),
        out.join("platform.Cargo.lock"),
    )?;
    let harts = selected_harts(&harts, &[1, 2, 4])?;
    input_json["image_profile"] = json!("release");
    record_hashes(
        &mut input_json,
        &[
            kernel.clone(),
            image.clone(),
            guest_elf.clone(),
            image_elf.clone(),
            out.join("guest.Cargo.lock"),
            out.join("image.Cargo.lock"),
            out.join("platform.Cargo.lock"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"),
        ],
    )?;
    json(&out.join("inputs.json"), &input_json)?;
    for hart in harts {
        let dir = out.join(format!("harts-{hart}"));
        let outcome = run_qemu(&kernel, &image, hart, &dir, Duration::from_secs(120), true)?;
        checked_run(
            outcome,
            &dir,
            &[SYSTEM_MARKER, marker, "task: all tasks exited"],
            &["FAIL", "page fault killed", "[kernel] fatal"],
        )?;
    }
    Ok(())
}
