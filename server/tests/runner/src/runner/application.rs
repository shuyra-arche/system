use super::execution::{
    begin, build, effective_harts, evidence, logs, profile, record_hashes, root, run_command,
    run_qemu, selected_harts,
};
use crate::options::Application;
use kernel_verification::{Result, require, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::Command,
    time::Duration,
};

const HANDOFF_SCENARIOS: [&str; 11] = [
    "handoff",
    "auth",
    "cancel",
    "death",
    "owner",
    "owner-cancel",
    "owner-death",
    "supply",
    "supply-auth",
    "install",
    "install-loss",
];

pub(crate) fn run(_args: Application, _handoff: bool) -> Result<()> {
    application_run(_args, _handoff)
}

fn application_run(args: Application, handoff: bool) -> Result<()> {
    let common = args.common;
    let (out, kernel, kernel_sha) = begin(&common)?;
    let harts = effective_harts(&common.harts, &[1]);
    let root = root();
    let target = out.join("system-target");
    let system_manifest = root.join("Cargo.toml");
    let pack_manifest = root.join("server/tests/target/app/pack/Cargo.toml");
    let app_root = root.join("server/tests/target");
    let mode = if handoff { "handoff" } else { "application" };
    let debug_handoff = handoff && args.kernel_profile == "debug";
    let timeout = if debug_handoff { 180 } else { 90 };
    let role_budget = if debug_handoff { 120 } else { 60 };
    let system_profile = profile(&common.profile);
    let mut system_args = vec!["-p", "system-server", "--bin", "system"];
    system_args.extend(system_profile);
    build(
        &system_manifest,
        TARGET,
        &system_args,
        &[],
        &target,
        &[],
        &out.join("build-system"),
    )?;
    let system_name = if common.profile == "release" {
        "release"
    } else {
        "debug"
    };
    let system = target.join(TARGET).join(system_name).join("system");
    check_system_stack(&fs::read(&system)?)?;

    let scenarios: Vec<String> = if !args.scenarios.is_empty() {
        args.scenarios
    } else if handoff {
        HANDOFF_SCENARIOS.iter().map(|s| (*s).to_owned()).collect()
    } else {
        vec!["0".into(), "2".into(), "1".into()]
    };
    if !handoff {
        require(
            scenarios
                .iter()
                .all(|s| ["0", "1", "2"].contains(&s.as_str())),
            "application scenarios must be 0, 1, or 2",
        )?;
    } else {
        let accepted = [
            "handoff",
            "auth",
            "cancel",
            "death",
            "owner",
            "owner-cancel",
            "owner-death",
            "supply",
            "supply-auth",
            "supply-contend",
            "install",
            "install-loss",
        ];
        require(
            scenarios.iter().all(|s| accepted.contains(&s.as_str())),
            "unknown handoff scenario",
        )?;
    }
    require(
        scenarios.iter().collect::<BTreeSet<_>>().len() == scenarios.len(),
        "scenario names must be unique",
    )?;

    let user_manifest = if handoff {
        app_root.join("handoff/Cargo.toml")
    } else {
        app_root.join("loader/image/Cargo.toml")
    };
    let mut images = BTreeMap::new();
    for scenario in &scenarios {
        let kind = scenario_kind(scenario, handoff);
        if images.contains_key(kind) {
            continue;
        }
        let user_target = out.join(format!("user-{kind}"));
        let feature: Vec<&str> = match kind {
            "provider" => vec!["--features", "provider"],
            "install" => vec!["--features", "install"],
            _ => vec![],
        };
        let envs = if handoff {
            vec![(
                "SHUYRA_HANDOFF_BUDGET_SECONDS".to_owned(),
                role_budget.to_string(),
            )]
        } else {
            vec![]
        };
        build(
            &user_manifest,
            TARGET,
            &["--release"],
            &feature,
            &user_target,
            &envs,
            &out.join(format!("build-user-{kind}")),
        )?;
        let executable = if handoff {
            "system-handoff-guest"
        } else {
            "system-loader-image"
        };
        images.insert(
            kind.to_owned(),
            user_target.join(TARGET).join("release").join(executable),
        );
    }
    let harts = selected_harts(&harts, &[1, 2, 4])?;

    let capsule_image = capsule::system_initrd(&fs::read(&system)?)?;
    let boot_capsule = out.join("system.capsule");
    fs::write(&boot_capsule, &capsule_image[4096..])?;
    let firmware = env::var("QEMU_OPENSBI").unwrap_or_else(|_| "default".into());
    let qemu = env::var("QEMU_RISCV64").unwrap_or_else(|_| "qemu-system-riscv64".into());
    let mut inputs = evidence(&common, &out, &kernel, &kernel_sha, mode, &harts)?;
    inputs["kernel_profile"] = json!(args.kernel_profile);
    inputs["system_profile"] = json!(common.profile);
    inputs["user_profile"] = json!("release");
    inputs["timeout_seconds"] = json!(timeout);
    inputs["handoff_budget_seconds"] = json!(role_budget);
    inputs["scenarios"] = json!(scenarios);
    let mut user_evidence = serde_json::Map::new();
    for (kind, image) in &images {
        user_evidence.insert(kind.clone(), json!({
            "path": image,
            "sha256": digest(image)?,
            "feature": if kind == "install" { "install" } else if kind == "provider" { "provider" } else { "default" }
        }));
    }
    inputs["users"] = Value::Object(user_evidence);
    let mut hashes = vec![
        kernel.clone(),
        system.clone(),
        boot_capsule.clone(),
        root.join("Cargo.lock"),
        root.join("server/tests/target/runtime/platform/Cargo.lock"),
        pack_manifest.with_file_name("Cargo.lock"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"),
    ];
    hashes.extend(images.values().cloned());
    hashes.push(user_manifest.parent().unwrap().join("Cargo.lock"));
    record_hashes(&mut inputs, &hashes)?;
    json(&out.join("inputs.json"), &inputs)?;

    let pack_target = out.join("pack-target");
    for scenario in scenarios {
        let kind = scenario_kind(&scenario, handoff);
        let image = &images[kind];
        let safe = scenario.replace('/', "_");
        let archive = out.join(format!("archive-{safe}.bin"));
        let mut pack = Command::new("cargo");
        pack.args([
            "run",
            "--locked",
            "--release",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--manifest-path",
        ])
        .arg(&pack_manifest)
        .arg("--target-dir")
        .arg(&pack_target)
        .arg("--")
        .arg(&boot_capsule)
        .arg(&system)
        .arg(image)
        .arg(&archive)
        .arg(&scenario)
        .current_dir(&root);
        let mut pack_result = run_command(
            &mut pack,
            &out.join(format!("pack-{safe}")),
            Duration::from_secs(900),
        )?;
        pack_result.passed = pack_result.success();
        json(&out.join(format!("pack-{safe}/result.json")), &pack_result)?;
        require(
            pack_result.success(),
            format!("archive pack failed for {scenario}; see output"),
        )?;
        let image_hash = digest(image)?;
        let archive_hash = digest(&archive)?;
        for hart in harts.iter().copied() {
            let mut accepted = false;
            for attempt in 0..=usize::from(handoff) {
                let suffix = if attempt == 0 {
                    String::new()
                } else {
                    "-retry-1".into()
                };
                let dir = out.join(format!("{safe}-harts-{hart}{suffix}"));
                let outcome = run_qemu(
                    &kernel,
                    &archive,
                    hart,
                    &dir,
                    Duration::from_secs(timeout),
                    false,
                )?;
                let mut receipt = json!({
                    "scenario": scenario,
                    "mode": mode,
                    "harts": hart,
                    "attempt": attempt + 1,
                    "system_profile": common.profile,
                    "kernel_profile": args.kernel_profile,
                    "system_sha256": digest(&system)?,
                    "user_sha256": image_hash,
                    "archive_sha256": archive_hash,
                    "command": outcome.command,
                    "status": outcome.status,
                    "returncode": outcome.code,
                    "passed": false
                });
                if !outcome.success() {
                    json(&dir.join("result.json"), &receipt)?;
                    return Err(format!(
                        "QEMU failed or timed out for {scenario}/{hart}; evidence: {}",
                        dir.display()
                    )
                    .into());
                }
                let serial = fs::read(dir.join("serial.log"))?;
                let all_logs = logs(&dir)?;
                require(
                    !all_logs.contains("FAIL")
                        && !all_logs.contains("page fault killed")
                        && !all_logs.contains("system: panic")
                        && !all_logs.contains("[kernel] fatal"),
                    format!(
                        "failure evidence in {scenario}; evidence: {}",
                        dir.display()
                    ),
                )?;
                let expected_final = if !handoff && scenario == "1" {
                    "system: supervision failed (4)"
                } else {
                    "system: complete (0)"
                };
                require(
                    all_logs.contains("all tasks exited") && all_logs.contains(expected_final),
                    format!(
                        "missing terminal System evidence for {scenario}; evidence: {}",
                        dir.display()
                    ),
                )?;
                let checker =
                    run_app_checker(&pack_manifest, &pack_target, &dir, &serial, &scenario)?;
                if handoff && checker == 2 && attempt == 0 {
                    receipt["inconclusive"] = json!("kernel-console-loss");
                    json(&dir.join("result.json"), &receipt)?;
                    println!(
                        "INCONCLUSIVE {}: preserving evidence and qualifying one fresh VM",
                        dir.display()
                    );
                    continue;
                }
                require(
                    checker == 0,
                    format!(
                        "System-owned pack checker rejected {scenario}; evidence: {}",
                        dir.display()
                    ),
                )?;
                receipt["passed"] = json!(true);
                json(&dir.join("result.json"), &receipt)?;
                accepted = true;
                break;
            }
            require(
                accepted,
                format!("handoff remained inconclusive after one retry: {scenario}/{hart}"),
            )?;
        }
    }
    let _ = (firmware, qemu);
    Ok(())
}

fn scenario_kind(scenario: &str, handoff: bool) -> &'static str {
    if !handoff {
        "default"
    } else if matches!(scenario, "install" | "install-loss") {
        "install"
    } else if matches!(scenario, "supply" | "supply-auth" | "supply-contend") {
        "provider"
    } else {
        "default"
    }
}

fn run_app_checker(
    manifest: &Path,
    target: &Path,
    dir: &Path,
    serial: &[u8],
    scenario: &str,
) -> Result<i32> {
    let log = dir.join("serial.log");
    fs::write(&log, serial)?;
    let mut command = Command::new("cargo");
    command
        .args([
            "run",
            "--quiet",
            "--locked",
            "--release",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--manifest-path",
        ])
        .arg(manifest)
        .arg("--target-dir")
        .arg(target)
        .arg("--");
    command.arg("check").arg(&log).arg(scenario);
    let mut result = run_command(&mut command, &dir.join("check"), Duration::from_secs(60))?;
    result.passed = result.success();
    json(&dir.join("check/result.json"), &result)?;
    Ok(result.code.unwrap_or(-1))
}

fn check_system_stack(elf: &[u8]) -> Result<()> {
    use std::convert::TryInto;
    let read = |at: usize, width: usize| -> Result<u64> {
        let bytes = elf
            .get(at..at.checked_add(width).ok_or("ELF offset overflow")?)
            .ok_or("truncated ELF")?;
        let mut word = [0; 8];
        word[..width].copy_from_slice(bytes);
        Ok(u64::from_le_bytes(word))
    };
    require(
        elf.get(..6) == Some(b"\x7fELF\x02\x01"),
        "expected ELF64 little endian",
    )?;
    let table = read(32, 8)? as usize;
    let width = read(54, 2)? as usize;
    let count = read(56, 2)? as usize;
    require(
        width >= 56 && count <= 1024,
        "invalid System ELF program table",
    )?;
    let mut loads = Vec::new();
    for index in 0..count {
        let at = table
            .checked_add(index.checked_mul(width).ok_or("ELF table overflow")?)
            .ok_or("ELF table overflow")?;
        let ty = read(at, 4)?;
        let flags = read(at + 4, 4)?;
        let va = read(at + 16, 8)?;
        let file = read(at + 32, 8)?;
        let memory = read(at + 40, 8)?;
        if ty == 1 {
            loads.push((flags, va, file, memory));
        }
    }
    let stacks: Vec<_> = loads
        .iter()
        .filter(|(flags, _, file, memory)| *flags == 6 && *file == 0 && *memory >= 1024 * 1024)
        .collect();
    require(
        stacks.len() == 1,
        "missing distinct RW System lifetime stack",
    )?;
    let (_, address, _, memory) = *stacks[0];
    require(
        address % 4096 == 0 && memory % 4096 == 0 && address >= 4096,
        "unaligned System stack or missing guard page",
    )?;
    let guard = address - 4096;
    require(
        loads.iter().all(|(_, start, _, size)| {
            start
                .checked_add(*size)
                .is_some_and(|end| end <= guard || *start >= address)
        }),
        "a System PT_LOAD overlaps the stack guard",
    )?;
    let _ = TryInto::<[u8; 4]>::try_into([0; 4]);
    Ok(())
}
