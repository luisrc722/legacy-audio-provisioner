use lap_core::audio_discovery;
use lap_core::backup::BackupMetadata;
use lap_core::checkpoint::{
    write_json_atomically_to_paths, CheckpointData, CheckpointManager, FileCheckpoint,
    OperationStatus,
};
use lap_core::diffing;
use lap_core::distribution;
use lap_core::error::ProvisioningError;
use lap_core::hardware;
use lap_core::in_place_transformer::InPlaceTransformer;
use lap_core::ipc::IpcEvent;
use lap_core::normalizer;
use lap_core::sanitizer;
use lap_core::security;
use lap_core::verification;

use chrono::Utc;
use sha2::Digest;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
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

fn sha256_of_file(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn generate_valid_mp3(path: &Path, frequency: u32) -> anyhow::Result<()> {
    let output = Command::new("ffmpeg")
        .args([
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={}:duration=0.15", frequency),
            "-q:a",
            "9",
            "-acodec",
            "libmp3lame",
            "-y",
        ])
        .arg(path)
        .output()?;

    assert!(
        output.status.success(),
        "ffmpeg failed to generate fixture: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

fn generate_mp3_with_title_tag(input: &Path, output: &Path, title: &str) -> anyhow::Result<()> {
    let output_cmd = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-i"])
        .arg(input)
        .args(["-codec", "copy", "-metadata", &format!("title={title}")])
        .arg(output)
        .output()?;

    assert!(
        output_cmd.status.success(),
        "ffmpeg failed to inject title tag: stdout={} stderr={}",
        String::from_utf8_lossy(&output_cmd.stdout),
        String::from_utf8_lossy(&output_cmd.stderr)
    );

    Ok(())
}

fn ffprobe_has_title_tag(path: &Path) -> anyhow::Result<bool> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=title:stream_tags=title",
            "-of",
            "json",
        ])
        .arg(path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "ffprobe failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    Ok(text.contains("\"title\""))
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
fn test_00a_sanitize_filename_rules() {
    assert_eq!(sanitizer::sanitize_filename("valid_name.mp3"), "valid_name.mp3");
    assert_eq!(sanitizer::sanitize_filename("Canción.mp3"), "Cancin.mp3");
    assert_eq!(sanitizer::sanitize_filename("song🎵.mp3"), "song.mp3");
    assert_eq!(sanitizer::sanitize_filename("Tema + Remix = 2026.mp3"), "TemaRemix2026.mp3");
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
        std::slice::from_ref(&orphan_file),
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
        normalizer::ProcessingDecision::FfmpegTranscode,
    );
    let err = result.expect_err(".m4p should be rejected before ffprobe is required");
    let typed = err.downcast_ref::<ProvisioningError>();

    assert!(matches!(
        typed,
        Some(ProvisioningError::DrmProtected { .. })
    ));
}

#[test]
fn test_08a_m4p_with_shell_chars_in_source_is_not_blocked_by_r35() {
    let result = normalizer::normalize_audio(
        Path::new("/nonexistent/yo no fue pa' mi (official).m4p"),
        Path::new("/tmp/out.mp3"),
        normalizer::ProcessingDecision::FfmpegTranscode,
    );
    let err = result.expect_err(".m4p should be rejected as DRM even when source name is noisy");
    let typed = err.downcast_ref::<ProvisioningError>();

    assert!(matches!(
        typed,
        Some(ProvisioningError::DrmProtected { .. })
    ));
}

#[test]
fn test_08b_fast_in_place_never_enters_normalize_pipeline() {
    let result = normalizer::normalize_audio(
        Path::new("/nonexistent/clean_track.mp3"),
        Path::new("/tmp/out.mp3"),
        normalizer::ProcessingDecision::FastInPlaceRename,
    );

    let err = result.expect_err("FastInPlaceRename must fail-fast before invoking ffmpeg");
    assert!(
        err.to_string().contains("use fs::rename en el orquestador"),
        "unexpected error: {err}"
    );
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

    assert!(matches!(
        typed,
        Some(ProvisioningError::HardwareFraudDetected { .. })
    ));
    Ok(())
}

#[test]
fn test_11_path_traversal_is_rejected() -> anyhow::Result<()> {
    let base = TempDir::new()?;
    let result = security::validate_path_containment(base.path(), Path::new("../escape.mp3"));

    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_12_shell_injection_filename_is_rejected() {
    let result = security::validate_shell_safe_filename("track.mp3; rm -rf /");
    assert!(result.is_err());
}

#[test]
fn test_13_metadata_bomb_is_rejected() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let big_file = temp.path().join("oversized_tag.mp3");
    let payload = vec![0x41_u8; (security::MAX_ID3_TAG_SIZE + 1) as usize];
    fs::write(&big_file, payload)?;

    let result = security::validate_metadata_bomb_safety(&big_file, security::MAX_ID3_TAG_SIZE);
    assert!(result.is_err());

    Ok(())
}

#[cfg(unix)]
#[test]
fn test_14_preflight_rw_probe_fails_fast_on_read_only_target() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = temp.path().to_path_buf();

    let original_mode = fs::metadata(&path)?.permissions().mode();
    let mut readonly_permissions = fs::metadata(&path)?.permissions();
    readonly_permissions.set_mode(0o555);
    fs::set_permissions(&path, readonly_permissions)?;

    let result = hardware::run_preflight_rw_probe(&path);

    let mut restore_permissions = fs::metadata(&path)?.permissions();
    restore_permissions.set_mode(original_mode);
    fs::set_permissions(&path, restore_permissions)?;

    assert!(!path.join(".fat32_dirty_test").exists());

    match result {
        Err(ProvisioningError::ReadOnlyFilesystem { .. }) => Ok(()),
        other => panic!("Expected preflight ReadOnlyFilesystem, got {:?}", other),
    }
}

#[cfg(unix)]
#[test]
fn test_15_checkpoint_enospc_maps_to_storage_full() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let checkpoint_path = temp.path().join("checkpoint.json");

    let err = write_json_atomically_to_paths(&checkpoint_path, Path::new("/dev/full"), "{}")
        .expect_err("Expected /dev/full to trigger ENOSPC mapping");

    assert!(matches!(err, ProvisioningError::StorageFull { .. }));
    assert!(!checkpoint_path.exists());

    Ok(())
}

#[test]
fn test_17_execute_recovery_restores_only_invalid_entries() -> anyhow::Result<()> {
    let source_dir = TempDir::new()?;
    let backup_dir = TempDir::new()?;
    let usb_dir = TempDir::new()?;

    let source_keep = source_dir.path().join("keep.mp3");
    let source_recover = source_dir.path().join("recover.mp3");
    generate_valid_mp3(&source_keep, 440)?;
    generate_valid_mp3(&source_recover, 660)?;

    let volume_dir = usb_dir.path().join("VOL_01");
    fs::create_dir_all(&volume_dir)?;

    let usb_keep = volume_dir.join("001_keep.mp3");
    let usb_recover = volume_dir.join("002_recover.mp3");

    fs::copy(&source_keep, &usb_keep)?;
    fs::write(&usb_recover, b"corrupted-bytes")?;

    let keep_hash_before = sha256_of_file(&usb_keep)?;

    let mut checkpoint = CheckpointManager::new(
        backup_dir.path().to_path_buf(),
        usb_dir.path().to_path_buf(),
        source_dir.path().to_path_buf(),
        2,
    )?;

    checkpoint.record_file_start(
        0,
        source_keep.clone(),
        "001_keep.mp3".to_string(),
        sha256_of_file(&source_keep)?,
    )?;
    checkpoint.mark_file_completed(0, keep_hash_before.clone())?;

    checkpoint.record_file_start(
        1,
        source_recover.clone(),
        "002_recover.mp3".to_string(),
        sha256_of_file(&source_recover)?,
    )?;
    checkpoint.mark_file_completed(1, "0".repeat(64))?;

    let recovery_manager = lap_core::recovery::RecoveryManager::new(
        backup_dir.path().to_path_buf(),
        usb_dir.path().to_path_buf(),
    );
    recovery_manager.execute_recovery(&mut checkpoint)?;

    let keep_hash_after = sha256_of_file(&usb_keep)?;
    let recovered_hash = sha256_of_file(&usb_recover)?;
    let checkpoint_data = checkpoint.get_data();

    assert_eq!(
        keep_hash_before, keep_hash_after,
        "valid file should not be rewritten"
    );
    assert_eq!(
        checkpoint_data
            .processed_files
            .get(&1)
            .and_then(|file| file.usb_checksum.clone())
            .as_deref(),
        Some(recovered_hash.as_str())
    );
    assert_eq!(
        checkpoint_data
            .processed_files
            .get(&1)
            .map(|file| file.status),
        Some(OperationStatus::Completed)
    );

    Ok(())
}

#[test]
fn test_18_pre_eject_verification_accepts_valid_topology_and_hashes() -> anyhow::Result<()> {
    let usb_dir = TempDir::new()?;
    let backup_dir = TempDir::new()?;

    let volume_dir = usb_dir.path().join("VOL_01");
    fs::create_dir_all(&volume_dir)?;

    let file_a = volume_dir.join("001_song.mp3");
    let file_b = volume_dir.join("002_song.mp3");
    fs::write(&file_a, b"audio-a")?;
    fs::write(&file_b, b"audio-b")?;

    let mut checkpoint = CheckpointData::new(
        backup_dir.path().to_path_buf(),
        usb_dir.path().to_path_buf(),
        PathBuf::from("/fake/source"),
        2,
    );
    checkpoint.processed_files.insert(
        0,
        completed_checkpoint_file(0, "001_song.mp3", Some(&sha256_of_file(&file_a)?)),
    );
    checkpoint.processed_files.insert(
        1,
        completed_checkpoint_file(1, "002_song.mp3", Some(&sha256_of_file(&file_b)?)),
    );
    checkpoint.operation_status = OperationStatus::Completed;

    let report = verification::pre_eject_verification(usb_dir.path(), &checkpoint, false)?;

    assert!(report.success);
    assert_eq!(report.total_volumes, 1);
    assert_eq!(report.total_files, 2);
    assert!(report.errors.is_empty());

    Ok(())
}

/// [R-06-002] Post-Write Verification
/// Verifica que `verify_file_integrity` detecta hash mismatch tras corrupción del archivo en USB,
/// y acepta la verificación cuando los hashes coinciden.
#[test]
fn test_19_verify_file_integrity_detects_post_write_corruption() -> anyhow::Result<()> {
    let usb_dir = TempDir::new()?;
    let backup_dir = TempDir::new()?;

    let vol_dir = usb_dir.path().join("VOL_01");
    fs::create_dir_all(&vol_dir)?;

    let file_path = vol_dir.join("001_song.mp3");
    fs::write(&file_path, b"correct-audio-content")?;

    let real_hash = sha256_of_file(&file_path)?;

    let mut checkpoint_data = CheckpointData::new(
        backup_dir.path().to_path_buf(),
        usb_dir.path().to_path_buf(),
        PathBuf::from("/fake/source"),
        1,
    );
    checkpoint_data.processed_files.insert(
        0,
        completed_checkpoint_file(0, "001_song.mp3", Some(&real_hash)),
    );

    // Primera verificación: hash correcto → OK
    let ok_report = verification::verify_file_integrity(usb_dir.path(), &checkpoint_data)?;
    assert!(ok_report.success, "Deberia pasar con hash correcto");
    assert_eq!(ok_report.total_files, 1);
    assert!(ok_report.errors.is_empty());

    // Corromper el archivo en la "USB"
    fs::write(&file_path, b"CORRUPTED-BYTES-DIFFERENT")?;

    // Segunda verificación: hash ya no coincide → informe con error
    let err_report = verification::verify_file_integrity(usb_dir.path(), &checkpoint_data)?;
    assert!(!err_report.success, "Deberia fallar con archivo corrupto");
    assert!(
        err_report.errors.iter().any(|e| e.contains("001_song.mp3")),
        "El informe debe mencionar el archivo corrupto"
    );

    Ok(())
}

#[test]
fn test_20_root_topology_sweep_prevents_pre_eject_false_positives() -> anyhow::Result<()> {
    let usb_dir = TempDir::new()?;
    let backup_root = TempDir::new()?;

    let vol_dir = usb_dir.path().join("VOL_01");
    fs::create_dir_all(&vol_dir)?;
    let valid_file = vol_dir.join("001_song.mp3");
    fs::write(&valid_file, b"valid-audio")?;

    let rogue_file = usb_dir.path().join("cliente.txt");
    let rogue_dir = usb_dir.path().join("MISC");
    fs::write(&rogue_file, b"data")?;
    fs::create_dir_all(&rogue_dir)?;
    fs::write(rogue_dir.join("nested.bin"), b"payload")?;

    let mut backup_meta = BackupMetadata::new_with_base_dir(Some(backup_root.path()))?;
    let candidates = diffing::collect_non_whitelisted_root_entries(usb_dir.path())?;
    assert_eq!(candidates.len(), 2);

    let quarantine_report = diffing::quarantine_non_whitelisted_root_entries(
        usb_dir.path(),
        &candidates,
        &mut backup_meta,
        "topology_sweep",
    )?;
    assert!(quarantine_report.failed.is_empty());
    assert_eq!(quarantine_report.quarantined.len(), 2);
    assert!(backup_meta.file_count >= 2);

    let mut checkpoint = CheckpointData::new(
        backup_root.path().to_path_buf(),
        usb_dir.path().to_path_buf(),
        PathBuf::from("/fake/source"),
        1,
    );
    checkpoint.processed_files.insert(
        0,
        completed_checkpoint_file(0, "001_song.mp3", Some(&sha256_of_file(&valid_file)?)),
    );
    checkpoint.operation_status = OperationStatus::Completed;

    let verification_report =
        verification::pre_eject_verification(usb_dir.path(), &checkpoint, false)?;
    assert!(verification_report.success);
    assert!(verification_report.errors.is_empty());
    assert_eq!(verification_report.total_volumes, 1);
    assert_eq!(verification_report.total_files, 1);

    Ok(())
}

#[test]
fn test_23_in_place_e2e_applies_fast_and_slow_paths() -> anyhow::Result<()> {
    let workspace = TempDir::new()?;
    let usb_root = workspace.path();

    // Archivo base limpio: PCM -> MP3 compatible via normalizador.
    let clean_wav = usb_root.join("clean_base.wav");
    let clean_root = usb_root.join("this_is_a_very_long_clean_filename_for_fast_path.mp3");
    generate_valid_mp3(&clean_wav, 700)?;
    normalizer::normalize_audio(
        &clean_wav,
        &clean_root,
        normalizer::ProcessingDecision::FfmpegTranscode,
    )?;
    fs::remove_file(&clean_wav)?;

    // Archivo sucio: inyecta tag TITLE para forzar ruta lenta de limpieza.
    let dirty_root = usb_root.join("dirty_song_with_metadata.mp3");
    generate_mp3_with_title_tag(&clean_root, &dirty_root, "dirty-title")?;

    let clean_before = sha256_of_file(&clean_root)?;
    let dirty_before = sha256_of_file(&dirty_root)?;

    assert!(
        ffprobe_has_title_tag(&dirty_root)?,
        "fixture dirty should contain TITLE tag before rebuild"
    );

    let dirty_decision = normalizer::classify_audio_processing(&dirty_root)?;
    assert!(
        dirty_decision != normalizer::ProcessingDecision::FastInPlaceRename,
        "dirty fixture should require ffmpeg path"
    );

    let plan = InPlaceTransformer::build_plan(usb_root)?;
    assert_eq!(plan.entries.len(), 2);

    let mut fast_count = 0usize;
    let mut slow_count = 0usize;
    let mut clean_after: Option<String> = None;
    let mut dirty_after: Option<String> = None;
    let mut dirty_output_path: Option<PathBuf> = None;
    let mut clean_output_path: Option<PathBuf> = None;

    for entry in plan.entries {
        if let Some(parent) = entry.destination_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Harness E2E: fuerza ruta rapida para un caso conocido y deja que la ruta
        // lenta se resuelva por clasificacion real para validar limpieza via ffmpeg.
        let decision = if entry.source_path == clean_root {
            normalizer::ProcessingDecision::FastInPlaceRename
        } else {
            normalizer::classify_audio_processing(&entry.source_path)?
        };
        match decision {
            normalizer::ProcessingDecision::FastInPlaceRename => {
                fs::rename(&entry.source_path, &entry.destination_path)?;
                fast_count += 1;
                clean_after = Some(sha256_of_file(&entry.destination_path)?);
                clean_output_path = Some(entry.destination_path.clone());
            }
            normalizer::ProcessingDecision::FfmpegCopyClean
            | normalizer::ProcessingDecision::FfmpegTranscode => {
                normalizer::normalize_audio(&entry.source_path, &entry.destination_path, decision)?;
                fs::remove_file(&entry.source_path)?;
                slow_count += 1;
                dirty_after = Some(sha256_of_file(&entry.destination_path)?);
                dirty_output_path = Some(entry.destination_path.clone());
            }
        }

        assert!(entry.volume_name.starts_with("VOL_"));
        assert!(entry.normalized_name.len() <= 32);
    }

    assert_eq!(fast_count, 1, "expected exactly one fast-path file");
    assert_eq!(slow_count, 1, "expected exactly one slow-path file");

    assert_eq!(clean_after.as_deref(), Some(clean_before.as_str()));
    assert_ne!(dirty_after.as_deref(), Some(dirty_before.as_str()));

    let cleaned_fast_path = clean_output_path.expect("fast destination path missing");
    let cleaned_dirty_path = dirty_output_path.expect("dirty destination path missing");
    assert!(cleaned_fast_path.exists());
    assert!(cleaned_dirty_path.exists());

    assert!(
        !ffprobe_has_title_tag(&cleaned_dirty_path)?,
        "dirty output should be stripped from TITLE metadata"
    );

    Ok(())
}
