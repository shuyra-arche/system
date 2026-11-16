use super::runner;
use std::{path::PathBuf, process::Command, thread, time::Duration};

fn scratch(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "system-runner-{label}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

#[test]
fn timeout_after_a_pass_marker_still_fails() {
    let dir = scratch("timeout-marker");
    let mut command = Command::new("sh");
    command.args(["-c", "echo 'PASS contract'; sleep 30"]);
    let outcome = runner::run_command(&mut command, &dir, Duration::from_millis(150)).unwrap();
    let log = runner::logs(&dir).unwrap();
    assert!(log.contains("PASS contract"));
    assert_eq!(outcome.status, "timeout");
    assert!(!runner::case_passes(
        &outcome,
        &log,
        &["PASS contract"],
        &[]
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_nonzero_exit_after_a_pass_marker_still_fails() {
    let dir = scratch("nonzero-marker");
    let mut command = Command::new("sh");
    command.args(["-c", "echo 'PASS contract'; exit 7"]);
    let outcome = runner::run_command(&mut command, &dir, Duration::from_secs(2)).unwrap();
    let log = runner::logs(&dir).unwrap();
    assert!(log.contains("PASS contract"));
    assert_eq!(outcome.code, Some(7));
    assert!(!runner::case_passes(
        &outcome,
        &log,
        &["PASS contract"],
        &[]
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_spawn_error_leaves_an_explicit_failed_receipt() {
    let dir = scratch("spawn-error");
    let mut command = Command::new(dir.join("missing-executable"));
    assert!(runner::run_command(&mut command, &dir, Duration::from_secs(2)).is_err());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(receipt["status"], "spawn-error");
    assert_eq!(receipt["passed"], false);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(target_os = "linux")]
#[test]
fn parent_cancellation_kills_a_direct_child() {
    let marker = scratch("orphan-marker");
    let dir = scratch("cancel-child");
    if let Some(marker) = std::env::var_os("SYSTEM_RUNNER_PARENT_DEATH_MARKER") {
        let child_dir = PathBuf::from(
            std::env::var_os("SYSTEM_RUNNER_PARENT_DEATH_DIR").expect("child dir is passed"),
        );
        let mut command = Command::new("sh");
        command.args([
            "-c",
            &format!(
                "echo child-started; sleep 3; touch '{}'",
                PathBuf::from(marker).display()
            ),
        ]);
        let _ = runner::run_command(&mut command, &child_dir, Duration::from_secs(10));
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "execution::parent_cancellation_kills_a_direct_child",
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
        std::fs::read_to_string(&serial).is_ok_and(|text| text.contains("child-started")),
        "child did not start"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    thread::sleep(Duration::from_millis(3200));
    assert!(
        !marker.exists(),
        "child continued after runner cancellation"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
