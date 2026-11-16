use crate::{Application, Common, Entry, Loader};
use kernel_verification::{capsule, process, *};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const SYSTEM_MARKER: &str = "PASS loader-elf native=1 user=1 arguments=1 private=1 cache=1 eviction=1 cancel=1 running-stop=1 journal=1 reclaim=1";
pub(crate) const RUNTIME_MARKERS: [&str; 4] = [
    "PASS system-runtime heap=1 growing=1 shared=1 owner=1 worker-exit=1 reclaim=1 contention=1",
    "PASS supply-native rights=1 scope=1 replay=1 cancel=1 reorder=1 withdrawal=1 cleanup=1",
    "PASS supply-window relative=1 narrowed=1 once=1 cleanup=1",
    "PASS system-rpc abi=13 protocol=4 context=1 sender=1 reply=1 cleanup=1 worker-reclaimed=1",
];
const ENTRY_MARKERS: [&str; 3] = [
    "PASS system-entry words=4 ledger=13 catalog=2 dtb=1 heap=1",
    "PASS entry-services heap=production roles=2 root=1 wired=1 retire=1",
    "PASS boot-data native=1 readonly=1 content=1 padding=1 retire=1",
];
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
pub(crate) const KERNEL_RUNNER_REV: &str = "789ffd19b5359f3de172fb9aad8a61f6a7113a64";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("runner is below server/tests")
        .to_path_buf()
}

fn profile(profile: &str) -> Vec<&'static str> {
    if profile == "release" {
        vec!["--release"]
    } else {
        vec![]
    }
}

fn begin(common: &Common) -> Result<(PathBuf, PathBuf, String)> {
    let kernel = common.kernel.canonicalize()?;
    require(kernel.is_file(), "kernel ELF is not a file")?;
    if let Some(expected) = &common.system_revision {
        let actual = revision(&root())?;
        require(
            pin_matches(expected, &actual),
            "System checkout revision does not match --system-revision",
        )?;
        require(
            !system_dirty()?,
            "pinned System checkout has uncommitted source changes",
        )?;
    }
    require(
        !common.output.exists(),
        "output path already exists; choose a new output directory",
    )?;
    if let Some(parent) = common.output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&common.output)?;
    let output = common.output.canonicalize()?;
    let kernel_sha = digest(&kernel)?;
    if let Some(expected) = &common.kernel_sha256 {
        require(
            hash_matches(expected, &kernel_sha),
            "Kernel ELF SHA-256 does not match --kernel-sha256",
        )?;
    }
    Ok((output, kernel, kernel_sha))
}

fn system_dirty() -> Result<bool> {
    Ok(!output(git(&root()).args(["status", "--porcelain", "--", ".", ":!.evidence"]))?.is_empty())
}

fn build(
    manifest: &Path,
    target: &str,
    selected: &[&str],
    extra: &[&str],
    target_dir: &Path,
    environment: &[(String, String)],
    log: &Path,
) -> Result<()> {
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("--locked")
        .args(selected)
        .args(["--target", target, "--manifest-path"])
        .arg(manifest)
        .arg("--target-dir")
        .arg(target_dir)
        .args(extra)
        .current_dir(root())
        .env("RUSTFLAGS", FLAGS)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .envs(environment.iter().cloned());
    let mut result = run_command(&mut command, log, Duration::from_secs(900))?;
    result.passed = result.success();
    json(&log.join("result.json"), &result)?;
    require(
        result.passed,
        format!("build failed; evidence: {}", log.display()),
    )
}

pub(crate) fn run_command(
    command: &mut Command,
    log: &Path,
    timeout: Duration,
) -> Result<process::Outcome> {
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        let parent = libc::getpid();
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "runner exited before child start",
                ));
            }
            Ok(())
        });
    }
    process::run(command, log, timeout)
}

pub(crate) fn case_passes(
    outcome: &process::Outcome,
    log: &str,
    markers: &[&str],
    forbidden: &[&str],
) -> bool {
    outcome.success()
        && markers
            .iter()
            .all(|marker| log.matches(marker).count() == 1)
        && forbidden.iter().all(|marker| !log.contains(marker))
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn test_outcome(status: &str, code: Option<i32>) -> process::Outcome {
    process::Outcome {
        command: "fixture".into(),
        status: status.into(),
        code,
        elapsed_seconds: 0.0,
        passed: false,
    }
}

pub(crate) fn pin_matches(expected: &str, actual: &str) -> bool {
    expected == actual && actual.len() == 40
}

pub(crate) fn hash_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        && actual.len() == 64
        && actual
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub(crate) fn logs(dir: &Path) -> Result<String> {
    let mut content = String::new();
    for file in ["serial.log", "stderr.log"] {
        let path = dir.join(file);
        require(
            path.is_file(),
            format!("required process log was lost: {}", path.display()),
        )?;
        content.push_str(&String::from_utf8_lossy(&fs::read(path)?));
        content.push('\n');
    }
    Ok(content)
}

fn evidence(
    common: &Common,
    _out: &Path,
    kernel: &Path,
    kernel_sha: &str,
    mode: &str,
) -> Result<Value> {
    let firmware = env::var("QEMU_OPENSBI").unwrap_or_else(|_| "default".into());
    let qemu = env::var("QEMU_RISCV64").unwrap_or_else(|_| "qemu-system-riscv64".into());
    let qemu_version = output(Command::new(&qemu).arg("--version"))?;
    let firmware_hash = Path::new(&firmware)
        .is_file()
        .then(|| digest(Path::new(&firmware)))
        .transpose()?;
    let value = json!({
        "mode": mode,
        "profile": common.profile,
        "kernel": kernel,
        "kernel_sha256": kernel_sha,
        "firmware": firmware,
        "firmware_sha256": firmware_hash,
        "qemu": qemu_version,
        "rustc": output(Command::new("rustc").arg("-Vv"))?,
        "cargo": output(Command::new("cargo").arg("-V"))?,
        "rustflags": FLAGS,
        "runner_sha256": digest(&env::current_exe()?)?,
        "kernel_verification_revision": KERNEL_RUNNER_REV,
        "runner_manifest_sha256": digest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))?,
        "runner_lock_sha256": digest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"))?,
        "system_revision": revision(&root())?,
        "system_dirty": system_dirty()?,
        "harts": common.harts,
        "sha256": {}
    });
    Ok(value)
}

fn record_hashes(inputs: &mut Value, files: &[PathBuf]) -> Result<()> {
    let hashes: BTreeMap<String, String> = files
        .iter()
        .map(|p| {
            require(
                p.is_file(),
                format!("required input is missing: {}", p.display()),
            )?;
            Ok((p.display().to_string(), digest(p)?))
        })
        .collect::<Result<_>>()?;
    inputs["sha256"] = serde_json::to_value(hashes)?;
    Ok(())
}

fn run_qemu(
    kernel: &Path,
    image: &Path,
    harts: u8,
    dir: &Path,
    timeout: Duration,
    multi_threaded_tcg: bool,
) -> Result<process::Outcome> {
    let qemu = env::var("QEMU_RISCV64").unwrap_or_else(|_| "qemu-system-riscv64".into());
    let firmware = env::var("QEMU_OPENSBI").unwrap_or_else(|_| "default".into());
    let mut command = Command::new(qemu);
    if multi_threaded_tcg {
        command.args(["-accel", "tcg,thread=multi"]);
    }
    command
        .args(["-M", "virt", "-cpu", "rv64", "-m", "128M", "-smp"])
        .arg(harts.to_string())
        .arg("-bios")
        .arg(firmware)
        .arg("-kernel")
        .arg(kernel)
        .arg("-initrd")
        .arg(image)
        .args(["-nographic", "-monitor", "none", "-nic", "none"]);
    run_command(&mut command, dir, timeout)
}

fn checked_run(
    mut outcome: process::Outcome,
    dir: &Path,
    markers: &[&str],
    forbidden: &[&str],
) -> Result<()> {
    let log = logs(dir)?;
    let ok = case_passes(&outcome, &log, markers, forbidden);
    outcome.passed = ok;
    json(&dir.join("result.json"), &outcome)?;
    require(
        ok,
        format!("scenario assertion failed; evidence: {}", dir.display()),
    )
}

pub fn runtime(args: Common) -> Result<()> {
    require(
        args.profile == "release",
        "runtime fixture preserves its release-only build",
    )?;
    let (out, kernel, kernel_sha) = begin(&args)?;
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
    let mut inputs = evidence(&args, &out, &kernel, &kernel_sha, "runtime")?;
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
    for hart in selected_harts(&args.harts, &[1, 4])? {
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

pub fn entry(args: Entry) -> Result<()> {
    let common = args.common;
    require(
        common.profile == "release",
        "entry fixture preserves its release-only build",
    )?;
    let (out, kernel, kernel_sha) = begin(&common)?;
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
    let mut inputs = evidence(&common, &out, &kernel, &kernel_sha, mode)?;
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
    for hart in selected_harts(&common.harts, &[1, 4])? {
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

fn selected_harts(given: &[u8], defaults: &[u8]) -> Result<Vec<u8>> {
    let selected = if given.is_empty() {
        defaults.to_vec()
    } else {
        given.to_vec()
    };
    require(
        selected.iter().all(|h| defaults.contains(h)),
        "Hart count is not qualified for this scenario",
    )?;
    require(!selected.is_empty(), "at least one Hart count is required")?;
    Ok(selected)
}

pub fn loader(args: Loader) -> Result<()> {
    let common = args.common;
    let (out, kernel, kernel_sha) = begin(&common)?;
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
    let mut input_json = evidence(&common, &out, &kernel, &kernel_sha, "loader")?;
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
    for hart in selected_harts(&common.harts, &[1, 2, 4])? {
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

pub fn application(_args: Application, _handoff: bool) -> Result<()> {
    application_run(_args, _handoff)
}

fn application_run(args: Application, handoff: bool) -> Result<()> {
    let common = args.common;
    let (out, kernel, kernel_sha) = begin(&common)?;
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

    let capsule_image = capsule::system_initrd(&fs::read(&system)?)?;
    let boot_capsule = out.join("system.capsule");
    fs::write(&boot_capsule, &capsule_image[4096..])?;
    let firmware = env::var("QEMU_OPENSBI").unwrap_or_else(|_| "default".into());
    let qemu = env::var("QEMU_RISCV64").unwrap_or_else(|_| "qemu-system-riscv64".into());
    let mut inputs = evidence(&common, &out, &kernel, &kernel_sha, mode)?;
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
        for hart in selected_harts(&common.harts, &[1, 2, 4])? {
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
