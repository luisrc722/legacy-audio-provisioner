use legacy_audio_provisioner::audio_discovery;
use legacy_audio_provisioner::backup::BackupMetadata;
use legacy_audio_provisioner::checkpoint::{
    CheckpointData, CheckpointManager, FileCheckpoint, OperationStatus,
};
use legacy_audio_provisioner::diffing;
use legacy_audio_provisioner::distribution;
use legacy_audio_provisioner::error::ProvisioningError;
use legacy_audio_provisioner::hardware;
use legacy_audio_provisioner::ipc::IpcEvent;
use legacy_audio_provisioner::normalizer;
use legacy_audio_provisioner::sanitizer;
use legacy_audio_provisioner::verification;

use chrono::Utc;
use std::collections::HashSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn create_test_audio_files(dir: &Path, count: usize) -> std::io::Result<Vec<PathBuf>> {
    fs::create_dir_all(dir)?;
    let mut files = Vec::new();

    for i in 1..=count {
        let filename = format!("song_{:03}_Tîtle_🎵.mp3", i);
        let filepath = dir.join(&filename);
        let content = format!("MP3 Header Simulation - File {}\n{}", i, "x".repeat(100));
        fs::write(&filepath, content)?;
        files.push(filepath);
    }

    Ok(files)
}

fn completed_checkpoint_file(
    index: usize,
    normalized_name: &str,
    usb_checksum: Option<&str>,
) -> FileCheckpoint {
    FileCheckpoint {
        original_path: PathBuf::from(format!("/fake/source_{:03}.mp3", index)),
        normalized_name: normalized_name.to_string(),
        status: OperationStatus::Completed,
        original_checksum: format!("original_hash_{:03}", index),
        usb_checksum: usb_checksum.map(|s| s.to_string()),
        start_time: Utc::now(),
        end_time: Some(Utc::now()),
        error_message: None,
    }
}

#[test]
fn test_00_system_dependencies() {
    let status = std::process::Command::new("which")
        .arg("ffmpeg")
        .output()
        .expect("Failed to execute 'which' command");

    if status.status.success() {
        println!("✅ ffmpeg installed");
    } else {
        println!("⚠️ ffmpeg missing. Audio normalization tests avoid requiring runtime ffmpeg.");
    }
}

#[test]
fn test_01_real_sanitization_and_distribution() -> anyhow::Result<()> {
    let long_name = format!("{}.mp3", "a".repeat(40));
    let test_files = [
        PathBuf::from("/fake/Canción_2024.mp3"),
        PathBuf::from(format!("/fake/{}", long_name)),
        PathBuf::from("/fake/song🎵.mp3"),
    ];

    let mappings: Vec<(PathBuf, String)> = test_files
        .iter()
        .enumerate()
        .map(|(idx, path)| {
            let name = path.file_name().unwrap().to_string_lossy();
            let sanitized = sanitizer::sanitize_filename(&name);
            let indexed = sanitizer::add_sequential_prefix(&sanitized, idx + 1);
            (path.clone(), indexed)
        })
        .collect();

    assert_eq!(mappings[0].1, "001_Cancin_2024.mp3");
    assert_eq!(mappings[1].1.len(), 32);
    assert!(mappings[1].1.ends_with(".mp3"));
    assert_eq!(mappings[2].1, "003_song.mp3");

    let mut bulk_mappings = Vec::new();
    for i in 0..125 {
        bulk_mappings.push((
            PathBuf::from(format!("/fake/{}.mp3", i)),
            format!("{:03}_file.mp3", i),
        ));
    }

    let volumes = distribution::plan_distribution(bulk_mappings)?;
    assert_eq!(volumes.len(), 3);
    assert_eq!(volumes[0].files.len(), 50);
    assert_eq!(volumes[1].files.len(), 50);
    assert_eq!(volumes[2].files.len(), 25);

    Ok(())
}

#[test]
fn test_02_real_audio_discovery() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;

    fs::write(temp_dir.path().join("song1.mp3"), "audio")?;
    fs::write(temp_dir.path().join("._song2.mp3"), "apple data")?;
    let trash = temp_dir.path().join(".Trash");
    fs::create_dir(&trash)?;
    fs::write(trash.join("deleted.mp3"), "audio")?;

    let report = audio_discovery::discover_audio_files(temp_dir.path())?;

    assert_eq!(report.total_files, 1);
    assert_eq!(report.audio_files[0].filename, "song1.mp3");

    Ok(())
}

#[test]
fn test_03_real_checkpoint_tracking() -> anyhow::Result<()> {
    let backup_dir = TempDir::new()?;
    let usb_dir = TempDir::new()?;

    let mut manager = CheckpointManager::new(
        backup_dir.path().to_path_buf(),
        usb_dir.path().to_path_buf(),
        PathBuf::from("/fake/source"),
        2,
    )?;

    manager.record_file_start(
        0,
        PathBuf::from("a.mp3"),
        "001_a.mp3".to_string(),
        "hash".to_string(),
    )?;
    manager.mark_file_completed(0, "usbhash".to_string())?;

    assert_eq!(manager.get_data().progress_percentage(), 50.0);
    assert!(backup_dir.path().join(".provisioning_checkpoint").exists());

    Ok(())
}

#[test]
fn test_04_end_to_end_backup_integration() -> anyhow::Result<()> {
    let source_dir = TempDir::new()?;
    let files = create_test_audio_files(source_dir.path(), 3)?;

    let backup_root = TempDir::new()?;
    let mut backup = BackupMetadata::new_with_base_dir(Some(backup_root.path()))?;

    for file in &files {
        backup.backup_file(file)?;
    }

    assert_eq!(backup.file_count, 3);
    assert!(backup.verify_backup()?);

    Ok(())
}

#[test]
fn test_05_sync_diff_ignores_existing_hashes() -> anyhow::Result<()> {
    let source = TempDir::new()?;
    let usb = TempDir::new()?;
    fs::create_dir_all(usb.path().join("VOL_01"))?;

    let existing_src = source.path().join("rola1.mp3");
    let new_src = source.path().join("nueva.mp3");
    let usb_existing = usb.path().join("VOL_01/001_rola1.mp3");

    fs::write(&existing_src, b"same-bytes")?;
    fs::write(&new_src, b"new-bytes")?;
    fs::write(&usb_existing, b"same-bytes")?;

    let source_files = audio_discovery::discover_audio_files(source.path())?.audio_files;
    let mut known_names = HashSet::new();
    known_names.insert("001_rola1.mp3".to_string());

    let report = diffing::calculate_sync_diff(&source_files, usb.path(), &known_names)?;

    assert_eq!(report.skipped_existing, 1);
    assert_eq!(report.files_to_process.len(), 1);
    assert!(report.files_to_process[0].path.ends_with("nueva.mp3"));
    assert_eq!(report.max_existing_index, 1);

    Ok(())
}

#[test]
fn test_06_orphan_isolation_to_quarantine() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let usb = tmp.path().join("usb");
    let backup_root = tmp.path().join("backup-root");
    fs::create_dir(&usb)?;
    fs::create_dir(&backup_root)?;

    let orphan_file = usb.join("foto_cliente_olvidada.jpg");
    fs::write(&orphan_file, b"datos importantes")?;

    let mut backup = BackupMetadata::new_with_base_dir(Some(&backup_root))?;
    let report = diffing::quarantine_untracked_files(
        &usb,
        &[orphan_file.clone()],
        &mut backup,
        "quarantine_test",
    )?;

    assert!(report.failed.is_empty());
    assert!(!orphan_file.exists());
    assert!(usb
        .join(".legacy_quarantine")
        .join("quarantine_test")
        .join("foto_cliente_olvidada.jpg")
        .exists());
    assert!(backup.file_count >= 1);

    Ok(())
}

#[test]
fn test_07_ipc_event_serialization_contract() {
    let event = IpcEvent::Progress {
        files_processed: 10,
        total_files: 100,
        percentage: 10.0,
        current_file: "test.mp3".to_string(),
        eta_seconds: 60,
    };

    let json = serde_json::to_string(&event).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["event"], "PROGRESS");
    assert_eq!(value["payload"]["files_processed"], 10);
    assert_eq!(value["payload"]["total_files"], 100);
    assert_eq!(value["payload"]["current_file"], "test.mp3");
}

#[test]
fn test_08_m4p_is_reported_as_drm_protected() {
    let result = normalizer::normalize_audio(
        Path::new("/nonexistent/protected_track.m4p"),
        Path::new("/tmp/out.mp3"),
    );
    let err = result.expect_err(".m4p should be rejected before ffprobe is required");
    let typed = err.downcast_ref::<ProvisioningError>();

    assert!(matches!(typed, Some(ProvisioningError::DrmProtected { .. })));
}

#[cfg(unix)]
#[test]
fn test_09_read_only_filesystem_maps_to_typed_error() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = temp.path().to_path_buf();

    let original_mode = fs::metadata(&path)?.permissions().mode();
    let mut readonly_permissions = fs::metadata(&path)?.permissions();
    readonly_permissions.set_mode(0o555);
    fs::set_permissions(&path, readonly_permissions)?;

    let result = hardware::assert_rw_filesystem(&path);

    let mut restore_permissions = fs::metadata(&path)?.permissions();
    restore_permissions.set_mode(original_mode);
    fs::set_permissions(&path, restore_permissions)?;

    match result {
        Err(ProvisioningError::ReadOnlyFilesystem { .. }) => Ok(()),
        other => panic!("Expected ReadOnlyFilesystem, got {:?}", other),
    }
}

#[test]
fn test_10_hardware_fraud_detected_after_five_hash_mismatches() -> anyhow::Result<()> {
    let usb = TempDir::new()?;
    fs::create_dir_all(usb.path().join("VOL_01"))?;

    let mut checkpoint = CheckpointData::new(
        PathBuf::from("/fake/backup"),
        usb.path().to_path_buf(),
        PathBuf::from("/fake/source"),
        5,
    );

    for index in 0..5usize {
        let file_name = format!("{:03}_track.mp3", index + 1);
        let file_path = usb.path().join("VOL_01").join(&file_name);
        fs::write(&file_path, format!("actual-data-{}", index))?;
        checkpoint.processed_files.insert(
            index,
            completed_checkpoint_file(index, &file_name, Some(&"0".repeat(64))),
        );
    }
    checkpoint.operation_status = OperationStatus::Completed;

    let err = verification::verify_file_integrity(usb.path(), &checkpoint)
        .expect_err("Expected fraud detection after five consecutive hash mismatches");
    let typed = err.downcast_ref::<ProvisioningError>();

    assert!(matches!(typed, Some(ProvisioningError::HardwareFraudDetected { .. })));
    Ok(())
}
