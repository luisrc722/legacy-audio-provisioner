use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn find_provisioning_log(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("provisioning.log") {
                return Some(path);
            }
        }
    }

    None
}

#[test]
fn test_16_session_log_is_created_with_json_entries() -> anyhow::Result<()> {
    let audio_root = TempDir::new()?;
    let log_root = TempDir::new()?;
    fs::write(audio_root.path().join("song.mp3"), b"fake audio payload")?;

    let binary = env!("CARGO_BIN_EXE_lap-bin-provision");
    let output = Command::new(binary)
        .env("LAP_LOG_DIR", log_root.path())
        .arg("scan")
        .arg("--usb")
        .arg(audio_root.path())
        .output()?;

    assert!(
        output.status.success(),
        "scan command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let log_path = find_provisioning_log(log_root.path()).expect("provisioning.log not found");
    let content = fs::read_to_string(&log_path)?;
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    assert!(!lines.is_empty(), "expected structured log entries");

    let entries: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line))
        .collect::<Result<_, _>>()?;

    assert!(entries
        .iter()
        .any(|entry| entry["operation"] == "SESSION_START"));
    assert!(entries
        .iter()
        .any(|entry| entry["operation"] == "COMMAND_START"));
    assert!(entries
        .iter()
        .any(|entry| entry["operation"] == "COMMAND_END"));
    assert!(entries.iter().all(|entry| entry.get("timestamp").is_some()));
    assert!(entries
        .iter()
        .all(|entry| entry.get("session_id").is_some()));
    assert!(entries
        .iter()
        .any(|entry| entry["operation"] == "COMMAND_END" && entry["status"] == "OK"));

    Ok(())
}

#[test]
fn test_21_json_ingest_emits_only_machine_readable_events() -> anyhow::Result<()> {
    let usb_source = TempDir::new()?;
    let staging = TempDir::new()?;

    fs::write(usb_source.path().join("song.mp3"), b"fake audio payload")?;

    let binary = env!("CARGO_BIN_EXE_lap-bin-provision");
    let output = Command::new(binary)
        .arg("--json")
        .arg("ingest")
        .arg("--usb")
        .arg(usb_source.path())
        .arg("--source")
        .arg(staging.path())
        .output()?;

    assert!(
        output.status.success(),
        "json ingest failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|line| !line.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "expected JSON events on stdout");

    let events: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line))
        .collect::<Result<_, _>>()?;

    assert!(events.iter().all(|e| e.get("event").is_some()));
    assert!(events.iter().any(|e| e["event"] == "PROGRESS"));
    assert!(events.iter().any(|e| e["event"] == "SUCCESS"));

    Ok(())
}

#[test]
fn test_22_json_scan_unsupported_feature_returns_typed_fatal_error() -> anyhow::Result<()> {
    let usb_root = TempDir::new()?;

    let binary = env!("CARGO_BIN_EXE_lap-bin-provision");
    let output = Command::new(binary)
        .arg("--json")
        .arg("scan")
        .arg("--usb")
        .arg(usb_root.path())
        .output()?;

    assert!(
        !output.status.success(),
        "expected unsupported json scan to fail: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|line| !line.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "expected JSON fatal error event");

    let events: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line))
        .collect::<Result<_, _>>()?;

    assert!(events.iter().any(|e| e["event"] == "FATAL_ERROR"));
    assert!(events.iter().any(|e| {
        e["event"] == "FATAL_ERROR" && e["payload"]["code"] == "UNSUPPORTED_JSON_MODE"
    }));

    Ok(())
}
