use crate::options::Common;
use kernel_verification::{process, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub(crate) const KERNEL_RUNNER_REV: &str = "789ffd19b5359f3de172fb9aad8a61f6a7113a64";
pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("runner is below server/tests")
        .to_path_buf()
}

pub(crate) fn profile(profile: &str) -> Vec<&'static str> {
    if profile == "release" {
        vec!["--release"]
    } else {
        vec![]
    }
}

pub(crate) fn begin(common: &Common) -> Result<(PathBuf, PathBuf, String)> {
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

pub(crate) fn system_dirty() -> Result<bool> {
    Ok(!output(git(&root()).args(["status", "--porcelain", "--", ".", ":!.evidence"]))?.is_empty())
}

pub(crate) fn build(
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
    return run_command_linux(command, log, timeout);

    #[cfg(not(target_os = "linux"))]
    process::run(command, log, timeout)
}

#[cfg(target_os = "linux")]
fn run_command_linux(
    command: &mut Command,
    log: &Path,
    timeout: Duration,
) -> Result<process::Outcome> {
    use std::{
        fs::OpenOptions,
        os::{fd::AsRawFd, unix::process::CommandExt},
    };

    fs::create_dir_all(log)?;
    let group_receipt = log.join("process-group.id");
    let receipt_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&group_receipt)?;
    let receipt_fd = receipt_file.as_raw_fd();
    if unsafe { libc::fcntl(receipt_fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let parent = unsafe { libc::getpid() };

    // This is process-wide and remains enabled so orphaned command descendants
    // are adopted here after their command leader exits.
    if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    unsafe {
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
            let group = libc::getpid();
            let bytes = group.to_ne_bytes();
            let written = libc::write(receipt_fd, bytes.as_ptr().cast(), bytes.len());
            if written != bytes.len() as isize {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let execution = process::run(command, log, timeout);
    drop(receipt_file);
    let group = fs::read(&group_receipt).ok().and_then(|bytes| {
        (bytes.len() == std::mem::size_of::<libc::pid_t>()).then(|| {
            let mut value = [0_u8; std::mem::size_of::<libc::pid_t>()];
            value.copy_from_slice(&bytes);
            libc::pid_t::from_ne_bytes(value)
        })
    });

    let cleanup = settle_process_group(group);
    json(&log.join("cleanup.json"), &cleanup)?;

    match execution {
        Ok(mut outcome) => {
            let had_descendants = cleanup["unexpected_descendants"] == true;
            let settled = cleanup["settled"] == true;
            if (had_descendants || !settled) && outcome.success() {
                outcome.status = "cleanup-failed".into();
                outcome.passed = false;
            }
            json(&log.join("result.json"), &outcome)?;
            Ok(outcome)
        }
        Err(error) => {
            if let Ok(mut receipt) =
                serde_json::from_slice::<Value>(&fs::read(log.join("result.json"))?)
            {
                receipt["passed"] = json!(false);
                if cleanup["settled"] != true {
                    receipt["cleanup_failed"] = json!(true);
                }
                json(&log.join("result.json"), &receipt)?;
            }
            Err(error)
        }
    }
}

#[cfg(target_os = "linux")]
fn process_group_exists(group: libc::pid_t) -> bool {
    if unsafe { libc::kill(-group, 0) } == 0 {
        true
    } else {
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

#[cfg(target_os = "linux")]
fn reap_process_group(group: libc::pid_t) -> Result<usize> {
    let mut reaped = 0;
    loop {
        let mut status = 0;
        let result = unsafe { libc::waitpid(-group, &mut status, libc::WNOHANG) };
        if result > 0 {
            reaped += 1;
        } else if result == 0 {
            break;
        } else {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            if error.raw_os_error() == Some(libc::ECHILD) {
                break;
            }
            return Err(error.into());
        }
    }
    Ok(reaped)
}

#[cfg(target_os = "linux")]
fn settle_process_group(group: Option<libc::pid_t>) -> Value {
    let Some(group) = group else {
        return json!({
            "process_group": null,
            "unexpected_descendants": false,
            "killed": false,
            "reaped": 0,
            "settled": true,
            "cleanup_seconds": 0.0,
            "error": null
        });
    };

    let started = Instant::now();
    let unexpected = process_group_exists(group);
    let mut killed = false;
    let mut reaped = 0;
    let mut error = None;
    if unexpected {
        killed = true;
        if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 {
            let kill_error = std::io::Error::last_os_error();
            if kill_error.raw_os_error() != Some(libc::ESRCH) {
                error = Some(kill_error.to_string());
            }
        }
    }

    let deadline = Instant::now() + Duration::from_secs(1);
    while process_group_exists(group) && Instant::now() < deadline {
        match reap_process_group(group) {
            Ok(count) => reaped += count,
            Err(reap_error) => {
                error = Some(reap_error.to_string());
                break;
            }
        }
        if process_group_exists(group) {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let settled = !process_group_exists(group);
    if !settled && error.is_none() {
        error = Some("process group did not settle within 1 second".into());
    }

    json!({
        "process_group": group,
        "unexpected_descendants": unexpected,
        "killed": killed,
        "reaped": reaped,
        "settled": settled,
        "cleanup_seconds": started.elapsed().as_secs_f64(),
        "error": error
    })
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

pub(crate) fn evidence(
    common: &Common,
    _out: &Path,
    kernel: &Path,
    kernel_sha: &str,
    mode: &str,
    harts: &[u8],
) -> Result<Value> {
    let firmware = env::var("QEMU_OPENSBI").unwrap_or_else(|_| "default".into());
    let qemu = env::var("QEMU_RISCV64").unwrap_or_else(|_| "qemu-system-riscv64".into());
    let qemu_binary = executable_path(&qemu)?;
    let qemu_version = output(Command::new(&qemu).arg("--version"))?;
    let (firmware_source, firmware_identity, firmware_path, firmware_hash) =
        if firmware == "default" {
            (
                "qemu-default",
                "qemu-default-unresolved".to_owned(),
                None,
                None,
            )
        } else {
            let path = Path::new(&firmware);
            require(
                path.is_file(),
                "QEMU_OPENSBI must be 'default' or a file that can be hashed",
            )?;
            let path = path.canonicalize()?;
            (
                "file",
                path.display().to_string(),
                Some(path.clone()),
                Some(digest(&path)?),
            )
        };
    let cpu_max_path = Path::new("/sys/fs/cgroup/cpu.max");
    let cpu_max = fs::read_to_string(cpu_max_path)
        .ok()
        .map(|contents| contents.trim().to_owned());
    let available_parallelism = std::thread::available_parallelism()
        .ok()
        .map(|count| count.get());
    let value = json!({
        "mode": mode,
        "profile": common.profile,
        "host_os": std::env::consts::OS,
        "host_arch": std::env::consts::ARCH,
        "cpu_max_path": cpu_max_path,
        "cpu_max": cpu_max,
        "available_parallelism": available_parallelism,
        "kernel": kernel,
        "kernel_sha256": kernel_sha,
        "firmware": firmware,
        "firmware_source": firmware_source,
        "firmware_identity": firmware_identity,
        "firmware_path": firmware_path,
        "firmware_sha256": firmware_hash,
        "qemu_binary": qemu_binary,
        "qemu_binary_sha256": digest(&qemu_binary)?,
        "qemu": qemu_version,
        "rustc": output(Command::new("rustc").arg("-Vv"))?,
        "cargo": output(Command::new("cargo").arg("-V"))?,
        "rustflags": FLAGS,
        "rust_toolchain_manifest_sha256": digest(&root().join("rust-toolchain.toml"))?,
        "runner_sha256": digest(&env::current_exe()?)?,
        "kernel_verification_revision": KERNEL_RUNNER_REV,
        "runner_manifest_sha256": digest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))?,
        "runner_lock_sha256": digest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"))?,
        "system_revision": revision(&root())?,
        "system_dirty": system_dirty()?,
        "harts": harts,
        "sha256": {}
    });
    Ok(value)
}

fn executable_path(program: &str) -> Result<PathBuf> {
    let program_path = Path::new(program);
    let candidate = if program_path.components().count() > 1 {
        program_path.to_path_buf()
    } else {
        let path = env::var_os("PATH").ok_or("PATH is not set")?;
        env::split_paths(&path)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| format!("could not resolve executable {program:?} in PATH"))?
    };
    let candidate = candidate.canonicalize()?;
    require(
        candidate.is_file(),
        format!("QEMU executable is not a file: {}", candidate.display()),
    )?;
    Ok(candidate)
}

pub(crate) fn record_hashes(inputs: &mut Value, files: &[PathBuf]) -> Result<()> {
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

pub(crate) fn run_qemu(
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

pub(crate) fn checked_run(
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

pub(crate) fn selected_harts(given: &[u8], defaults: &[u8]) -> Result<Vec<u8>> {
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
    require(
        selected.iter().copied().collect::<BTreeSet<_>>().len() == selected.len(),
        "Hart counts must be unique",
    )?;
    Ok(selected)
}

pub(crate) fn effective_harts(given: &[u8], defaults: &[u8]) -> Vec<u8> {
    if given.is_empty() {
        defaults.to_vec()
    } else {
        given.to_vec()
    }
}
