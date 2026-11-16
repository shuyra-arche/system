#![allow(dead_code)]

use std::path::PathBuf;

#[derive(Clone)]
struct Common {
    kernel: PathBuf,
    output: PathBuf,
    profile: String,
    harts: Vec<u8>,
    kernel_sha256: Option<String>,
    system_revision: Option<String>,
}
struct Entry {
    common: Common,
    abort: bool,
    stall: bool,
}
struct Loader {
    common: Common,
    toolchain_receipt: Option<PathBuf>,
    toolchain_fixture: String,
}
struct Application {
    common: Common,
    kernel_profile: String,
    scenarios: Vec<String>,
}

#[path = "../runner.rs"]
mod runner;

#[test]
fn a_case_requires_normal_exit_all_markers_and_no_failure_evidence() {
    let pass = runner::test_outcome("exited", Some(0));
    assert!(runner::case_passes(
        &pass,
        "PASS alpha PASS beta",
        &["PASS alpha", "PASS beta"],
        &["FAIL"]
    ));
    assert!(!runner::case_passes(
        &runner::test_outcome("timeout", None),
        "PASS alpha PASS beta",
        &["PASS alpha", "PASS beta"],
        &[]
    ));
    assert!(!runner::case_passes(
        &runner::test_outcome("exited", Some(7)),
        "PASS alpha PASS beta",
        &["PASS alpha", "PASS beta"],
        &[]
    ));
    assert!(!runner::case_passes(
        &pass,
        "PASS alpha",
        &["PASS alpha", "PASS beta"],
        &[]
    ));
    assert!(!runner::case_passes(
        &pass,
        "PASS alpha PASS beta FAIL cleanup",
        &["PASS alpha", "PASS beta"],
        &["FAIL"]
    ));
    assert!(!runner::case_passes(
        &pass,
        "PASS alpha PASS alpha PASS beta",
        &["PASS alpha", "PASS beta"],
        &[]
    ));
}

#[test]
fn a_missing_or_truncated_log_cannot_pass_by_exit_status() {
    let pass = runner::test_outcome("exited", Some(0));
    for log in ["", "PASS system-runtime heap=1 growing="] {
        assert!(!runner::case_passes(
            &pass,
            log,
            &runner::RUNTIME_MARKERS,
            &["FAIL"]
        ));
    }
}

#[test]
fn loss_of_either_qemu_stream_is_an_error() {
    let dir = std::env::temp_dir().join(format!("system-runner-log-loss-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("serial.log"), "all tasks exited").unwrap();
    assert!(runner::logs(&dir).is_err());
    std::fs::write(dir.join("stderr.log"), "").unwrap();
    assert!(runner::logs(&dir).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn receipt_and_kernel_runner_pins_are_exact() {
    assert_eq!(
        runner::KERNEL_RUNNER_REV,
        "789ffd19b5359f3de172fb9aad8a61f6a7113a64"
    );
    assert!(runner::pin_matches(
        "789ffd19b5359f3de172fb9aad8a61f6a7113a64",
        "789ffd19b5359f3de172fb9aad8a61f6a7113a64"
    ));
    assert!(!runner::pin_matches(
        "789ffd19b5359f3de172fb9aad8a61f6a7113a64",
        "0123456789abcdef0123456789abcdef01234567"
    ));
}

#[test]
fn kernel_hash_requires_exact_sha256_width_and_lowercase_hex() {
    let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(runner::hash_matches(hash, hash));
    assert!(!runner::hash_matches(&hash[..40], hash));
    assert!(!runner::hash_matches(&hash.to_uppercase(), hash));
    assert!(!runner::hash_matches(hash, "bad"));
}

#[cfg(target_os = "linux")]
#[test]
fn parent_cancellation_kills_the_qemu_like_child_before_success() {
    use std::{process::Command, thread, time::Duration};
    let marker = std::env::temp_dir().join(format!("system-runner-orphan-{}", std::process::id()));
    let dir = std::env::temp_dir().join(format!("system-runner-child-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(marker_path) = std::env::var_os("SYSTEM_RUNNER_PARENT_DEATH_MARKER") {
        let child_dir = std::env::var_os("SYSTEM_RUNNER_PARENT_DEATH_DIR").unwrap();
        let child_dir = PathBuf::from(child_dir);
        let mut command = Command::new("sh");
        command.args([
            "-c",
            &format!(
                "echo child-started; sleep 3; touch '{}'",
                marker_path.to_string_lossy()
            ),
        ]);
        let _ = runner::run_command(&mut command, &child_dir, Duration::from_secs(10));
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "parent_cancellation_kills_the_qemu_like_child_before_success",
        ])
        .env("SYSTEM_RUNNER_PARENT_DEATH_MARKER", &marker)
        .env("SYSTEM_RUNNER_PARENT_DEATH_DIR", &dir)
        .spawn()
        .unwrap();
    let serial = dir.join("serial.log");
    for _ in 0..100 {
        if std::fs::read_to_string(&serial).is_ok_and(|text| text.contains("child-started")) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        std::fs::read_to_string(&serial)
            .unwrap()
            .contains("child-started"),
        "child did not start"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    thread::sleep(Duration::from_millis(3200));
    assert!(
        !marker.exists(),
        "child continued after runner cancellation"
    );
    let _ = std::fs::remove_file(marker);
    let _ = std::fs::remove_dir_all(dir);
}
