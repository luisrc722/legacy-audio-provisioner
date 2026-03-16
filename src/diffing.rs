/// Diffing incremental USB vs source (R-23/R-26).
///
/// Implementacion segun ADR-0005 (`docs/adr/0005-sync-sha256.md`).
///
/// Objetivo:
/// - Detectar archivos nuevos en origen (no presentes por hash en USB)
/// - Detectar archivos huérfanos en USB (no registrados en checkpoint)
/// - Reconstruir estado incremental: máximo índice global y ocupación por VOL_XX

use crate::audio_discovery::{discover_audio_files, AudioFile};
use crate::backup::BackupMetadata;
use crate::distribution::{DistributedFile, VolumeSegment, MAX_FILES_PER_FOLDER};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SyncDiffReport {
    pub files_to_process: Vec<AudioFile>,
    pub skipped_existing: usize,
    pub untracked_in_target: Vec<PathBuf>,
    pub existing_volume_counts: BTreeMap<usize, usize>,
    pub max_existing_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct QuarantineReport {
    pub quarantined: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

fn unique_destination(base_dir: &Path, file_name: &str) -> PathBuf {
    let mut dest = base_dir.join(file_name);
    if !dest.exists() {
        return dest;
    }

    let file_path = Path::new(file_name);
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let mut i = 1usize;
    loop {
        let candidate = if ext.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, ext)
        };
        dest = base_dir.join(candidate);
        if !dest.exists() {
            return dest;
        }
        i += 1;
    }
}

/// R-25/R-26: aislamiento seguro de archivos no rastreados.
///
/// Reglas:
/// - Siempre realiza backup local primero (host) antes de mutar la USB.
/// - Mueve a `.legacy_quarantine/<session_label>/` para mantener la USB limpia
///   sin borrar datos del cliente.
pub fn quarantine_untracked_files(
    usb_mount: &Path,
    untracked_files: &[PathBuf],
    backup_meta: &mut BackupMetadata,
    session_label: &str,
) -> Result<QuarantineReport> {
    let quarantine_dir = usb_mount.join(".legacy_quarantine").join(session_label);
    fs::create_dir_all(&quarantine_dir)?;

    let mut report = QuarantineReport::default();

    for file in untracked_files {
        let source_path = if file.is_absolute() {
            file.clone()
        } else {
            usb_mount.join(file)
        };

        if !source_path.exists() {
            report
                .failed
                .push((source_path.clone(), "Archivo no existe".to_string()));
            continue;
        }

        if let Err(e) = backup_meta.backup_file(&source_path) {
            report.failed.push((
                source_path.clone(),
                format!("Backup preventivo fallido: {}", e),
            ));
            continue;
        }

        let file_name = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("orphan_audio.mp3");
        let dest_path = unique_destination(&quarantine_dir, file_name);

        match fs::rename(&source_path, &dest_path) {
            Ok(_) => report.quarantined.push(dest_path),
            Err(e) => report.failed.push((
                source_path,
                format!("Movimiento a cuarentena fallido: {}", e),
            )),
        }
    }

    if let Ok(dir) = File::open(&quarantine_dir) {
        let _ = dir.sync_all();
    }
    if let Ok(root) = File::open(usb_mount) {
        let _ = root.sync_all();
    }

    Ok(report)
}

fn parse_volume_index(path: &Path) -> Option<usize> {
    let parent = path.parent()?.file_name()?.to_string_lossy();
    if !parent.starts_with("VOL_") {
        return None;
    }
    parent.trim_start_matches("VOL_").parse::<usize>().ok()
}

fn parse_global_prefix_index(file_name: &str) -> Option<usize> {
    let (prefix, _) = file_name.split_once('_')?;
    prefix.parse::<usize>().ok()
}

fn compute_sha256(file_path: &Path) -> Result<String> {
    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
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

fn build_target_hash_index(target_mount: &Path) -> Result<(HashSet<String>, Vec<AudioFile>)> {
    let target_report = discover_audio_files(target_mount)?;
    let mut target_hashes = HashSet::new();

    for file in &target_report.audio_files {
        let hash = compute_sha256(&file.path)?;
        target_hashes.insert(hash);
    }

    Ok((target_hashes, target_report.audio_files))
}

pub fn calculate_sync_diff(
    source_files: &[AudioFile],
    target_mount: &Path,
    checkpoint_known_names: &HashSet<String>,
) -> Result<SyncDiffReport> {
    let (target_hashes, target_files) = build_target_hash_index(target_mount)?;

    let mut files_to_process = Vec::new();
    let mut skipped_existing = 0usize;

    for source in source_files {
        let src_hash = compute_sha256(&source.path)?;
        if target_hashes.contains(&src_hash) {
            skipped_existing += 1;
        } else {
            files_to_process.push(source.clone());
        }
    }

    let mut existing_volume_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut max_existing_index = 0usize;
    let mut untracked_in_target = Vec::new();

    for target in target_files {
        if let Some(vol_idx) = parse_volume_index(&target.path) {
            *existing_volume_counts.entry(vol_idx).or_insert(0) += 1;
        }

        if let Some(idx) = parse_global_prefix_index(&target.filename) {
            if idx > max_existing_index {
                max_existing_index = idx;
            }
        }

        if !checkpoint_known_names.is_empty() && !checkpoint_known_names.contains(&target.filename)
        {
            untracked_in_target.push(target.path.clone());
        }
    }

    Ok(SyncDiffReport {
        files_to_process,
        skipped_existing,
        untracked_in_target,
        existing_volume_counts,
        max_existing_index,
    })
}

pub fn plan_incremental_distribution(
    file_mappings: Vec<(PathBuf, String)>,
    existing_volume_counts: &BTreeMap<usize, usize>,
) -> Vec<VolumeSegment> {
    if file_mappings.is_empty() {
        return Vec::new();
    }

    let mut new_segments: HashMap<usize, VolumeSegment> = HashMap::new();
    let mut current_volume = existing_volume_counts
        .keys()
        .next_back()
        .copied()
        .unwrap_or(1);
    let mut current_count = existing_volume_counts.get(&current_volume).copied().unwrap_or(0);

    if current_count >= MAX_FILES_PER_FOLDER {
        current_volume += 1;
        current_count = 0;
    }

    for (source_path, sanitized_name) in file_mappings {
        if current_count >= MAX_FILES_PER_FOLDER {
            current_volume += 1;
            current_count = 0;
        }

        let segment = new_segments
            .entry(current_volume)
            .or_insert_with(|| VolumeSegment::new(current_volume));

        segment.add_file(DistributedFile {
            source_path,
            sanitized_name,
        });

        current_count += 1;
    }

    let mut volumes: Vec<(usize, VolumeSegment)> = new_segments.into_iter().collect();
    volumes.sort_by_key(|(idx, _)| *idx);
    volumes.into_iter().map(|(_, seg)| seg).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_plan_incremental_distribution_fills_last_volume() {
        let mut existing = BTreeMap::new();
        existing.insert(1, 50);
        existing.insert(2, 48);

        let mappings = vec![
            (PathBuf::from("a.mp3"), "051_a.mp3".to_string()),
            (PathBuf::from("b.mp3"), "052_b.mp3".to_string()),
            (PathBuf::from("c.mp3"), "053_c.mp3".to_string()),
        ];

        let planned = plan_incremental_distribution(mappings, &existing);

        assert_eq!(planned.len(), 2);
        assert_eq!(planned[0].folder_name, "VOL_02");
        assert_eq!(planned[0].files.len(), 2);
        assert_eq!(planned[1].folder_name, "VOL_03");
        assert_eq!(planned[1].files.len(), 1);
    }

    #[test]
    fn test_calculate_sync_diff_skips_existing_by_hash() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("VOL_01"))?;

        let src_existing = source.path().join("song_a.mp3");
        let src_new = source.path().join("song_b.mp3");
        let usb_existing = usb.path().join("VOL_01/001_song_a.mp3");

        fs::write(&src_existing, b"same content")?;
        fs::write(&src_new, b"new content")?;
        fs::write(&usb_existing, b"same content")?;

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &HashSet::new())?;

        assert_eq!(report.skipped_existing, 1);
        assert_eq!(report.files_to_process.len(), 1);
        assert!(report.files_to_process[0].path.ends_with("song_b.mp3"));
        assert_eq!(report.max_existing_index, 1);

        Ok(())
    }

    #[test]
    fn test_quarantine_untracked_files_backup_first() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;
        let backup_root = TempDir::new()?;

        let orphan = usb.path().join("VOL_01").join("manual_song.mp3");
        fs::create_dir_all(orphan.parent().unwrap())?;
        fs::write(&orphan, b"orphan-data")?;

        let mut backup_meta = BackupMetadata::new_with_base_dir(Some(backup_root.path()))?;
        let report = quarantine_untracked_files(
            usb.path(),
            &[orphan.clone()],
            &mut backup_meta,
            "sync_test",
        )?;

        assert_eq!(report.failed.len(), 0);
        assert_eq!(report.quarantined.len(), 1);
        assert!(!orphan.exists());
        assert!(report.quarantined[0].exists());
        assert!(backup_meta.file_count >= 1);

        let _ = source;
        Ok(())
    }
}
