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

mod execution;
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
    assert!(include_str!("../Cargo.lock").contains(
        "git+https://github.com/shuyra-arche/kernel.git?rev=789ffd19b5359f3de172fb9aad8a61f6a7113a64#789ffd19b5359f3de172fb9aad8a61f6a7113a64"
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
