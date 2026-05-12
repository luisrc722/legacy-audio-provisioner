/// Diffing incremental USB vs source (R-23/R-26).
///
/// Implementacion segun ADR-0005 (`docs/adr/0005-sync-sha256.md`).
///
/// Objetivo:
/// - Detectar archivos nuevos en origen (no presentes por hash en USB)
/// - Detectar archivos huérfanos en USB (no registrados en checkpoint)
/// - Reconstruir estado incremental: máximo índice global y ocupación por VOL_XX
use crate::audio_discovery::{discover_audio_files, visit_audio_files, AudioFile};
use crate::backup::BackupMetadata;
use crate::crypto::compute_file_sha256;
use crate::security::validate_path_containment;
use anyhow::Result;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

type TargetHashSet = HashSet<String>;
type DisplacedHashMap = HashMap<String, PathBuf>;
type TargetAudioFiles = Vec<AudioFile>;

#[derive(Debug, Clone)]
pub struct SyncDiffReport {
    pub files_to_process: Vec<AudioFile>,
    pub skipped_existing: usize,
    pub untracked_in_target: Vec<PathBuf>,
    pub displaced_in_target: HashMap<PathBuf, PathBuf>,
    pub existing_volume_counts: BTreeMap<usize, usize>,
    pub max_existing_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct QuarantineReport {
    pub quarantined: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootContentPolicy {
    Empty,
    ManagedTopology,
    PreserveUserContent,
}

#[derive(Debug, Clone)]
pub struct RootTopologyReport {
    pub policy: RootContentPolicy,
    pub non_whitelisted_entries: Vec<PathBuf>,
}

fn unique_destination(base_dir: &Path, file_name: &str) -> Result<PathBuf> {
    let mut dest = validate_path_containment(base_dir, Path::new(file_name))?;
    if !dest.exists() {
        return Ok(dest);
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
        dest = validate_path_containment(base_dir, Path::new(&candidate))?;
        if !dest.exists() {
            return Ok(dest);
        }
        i += 1;
    }
}

fn is_whitelisted_root_entry(name: &str) -> bool {
    name.starts_with("VOL_")
        || matches!(
            name,
            ".legacy_quarantine"
                | ".provisioning_checkpoint"
                | ".provisioning_checkpoint.tmp"
                | ".lap_provisioning.lock"
                | ".fat32_dirty_test"
                | "System Volume Information"
                | "$RECYCLE.BIN"
                | "LOST.DIR"
        )
}

/// [R-09-011] Barrido Universal de Raiz
/// Referencia legacy: extension operativa de R-09-006/R-09-010.
/// Precondicion: `usb_mount` apunta a la raiz del volumen objetivo validado.
/// Postcondicion: retorna todas las entradas raiz fuera de la whitelist operacional.
/// Invariante: cualquier nodo raiz no permitido debe ser identificable para aislamiento preventivo.
///
/// Escanea la raíz del volumen y devuelve entradas no permitidas por la topología legacy.
///
/// Política de whitelist:
/// - `VOL_XX`
/// - `.legacy_quarantine`
/// - `.provisioning_checkpoint` (+ tmp)
/// - `.lap_provisioning.lock`
/// - artefactos de sistema (`System Volume Information`, `$RECYCLE.BIN`, `LOST.DIR`)
pub fn collect_non_whitelisted_root_entries(usb_mount: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(usb_mount)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if is_whitelisted_root_entry(&name) {
            continue;
        }
        entries.push(entry.path());
    }

    entries.sort();
    Ok(entries)
}

pub fn analyze_root_topology(usb_mount: &Path) -> Result<RootTopologyReport> {
    let mut has_managed_markers = false;
    let mut non_whitelisted_entries = Vec::new();

    for entry in fs::read_dir(usb_mount)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        if matches!(
            name.as_str(),
            ".legacy_quarantine" | ".provisioning_checkpoint" | ".provisioning_checkpoint.tmp"
        ) || name.starts_with("VOL_")
        {
            has_managed_markers = true;
        }

        if is_whitelisted_root_entry(&name) {
            continue;
        }

        non_whitelisted_entries.push(entry.path());
    }

    non_whitelisted_entries.sort();

    let policy = if has_managed_markers {
        RootContentPolicy::ManagedTopology
    } else if non_whitelisted_entries.is_empty() {
        RootContentPolicy::Empty
    } else {
        RootContentPolicy::PreserveUserContent
    };

    Ok(RootTopologyReport {
        policy,
        non_whitelisted_entries,
    })
}

/// R-25/R-26: aislamiento seguro de archivos no rastreados.
///
/// Reglas:
/// Referencia legacy: R-25, R-26 (Aislamiento de Cuarentena).
/// - Siempre realiza respaldo local primero (host) antes de mutar la USB.
/// - Mueve a `.legacy_quarantine/<session_label>/` para mantener la USB limpia
/// - Auditoría: registro de que se encontraron X archivos no rastreados y fueron movidos a la zona de cuarentena.
///   sin borrar datos del cliente.
pub fn quarantine_untracked_files(
    usb_mount: &Path,
    untracked_files: &[PathBuf],
    backup_meta: &mut BackupMetadata,
    session_label: &str,
) -> Result<QuarantineReport> {
    let quarantine_base = validate_path_containment(usb_mount, Path::new(".legacy_quarantine"))?;
    let quarantine_dir = validate_path_containment(&quarantine_base, Path::new(session_label))?;
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
        let dest_path = match unique_destination(&quarantine_dir, file_name) {
            Ok(path) => path,
            Err(e) => {
                report.failed.push((
                    source_path.clone(),
                    format!("Violacion de seguridad en nombre de archivo (R-05): {}", e),
                ));
                continue;
            }
        };

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

/// [R-09-011] Barrido Universal de Raiz
/// Referencia legacy: extension operativa de R-09-006/R-09-010.
/// Precondicion: `entries` contiene nodos raiz fuera de whitelist en USB.
/// Postcondicion: aplica cuarentena backup-first sobre cada nodo (archivo o directorio) aislable.
/// Invariante: no se muta una entrada si su respaldo preventivo falla.
///
/// Aislamiento universal de topología en raíz USB (archivos y directorios completos).
///
/// Regla de seguridad: backup-first siempre. Si el backup falla, no se muta esa entrada.
pub fn quarantine_non_whitelisted_root_entries(
    usb_mount: &Path,
    entries: &[PathBuf],
    backup_meta: &mut BackupMetadata,
    session_label: &str,
) -> Result<QuarantineReport> {
    let quarantine_base = validate_path_containment(usb_mount, Path::new(".legacy_quarantine"))?;
    let quarantine_dir = validate_path_containment(&quarantine_base, Path::new(session_label))?;
    fs::create_dir_all(&quarantine_dir)?;

    let mut report = QuarantineReport::default();

    for entry in entries {
        if !entry.exists() {
            report
                .failed
                .push((entry.clone(), "Entrada no existe".to_string()));
            continue;
        }

        // Backup-first recursivo para directorios; simple para archivo.
        let backup_result: Result<()> = if entry.is_dir() {
            for walked in WalkDir::new(entry).into_iter().filter_map(|e| e.ok()) {
                let path = walked.path();
                if path.is_file() {
                    backup_meta.backup_file(path)?;
                }
            }
            Ok(())
        } else if entry.is_file() {
            backup_meta.backup_file(entry)
        } else {
            Ok(())
        };

        if let Err(e) = backup_result {
            report
                .failed
                .push((entry.clone(), format!("Backup preventivo fallido: {}", e)));
            continue;
        }

        let entry_name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("root_entry");
        let dest_path = match unique_destination(&quarantine_dir, entry_name) {
            Ok(path) => path,
            Err(e) => {
                report.failed.push((
                    entry.clone(),
                    format!("Violacion de seguridad en nombre de archivo (R-05): {}", e),
                ));
                continue;
            }
        };

        match fs::rename(entry, &dest_path) {
            Ok(_) => report.quarantined.push(dest_path),
            Err(e) => report.failed.push((
                entry.clone(),
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

fn has_legacy_safe_name(file_name: &str) -> bool {
    !file_name.is_empty()
        && file_name.len() <= 32
        && file_name.is_ascii()
        && file_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

fn is_legacy_compliant_target(path: &Path, file_name: &str) -> bool {
    parse_volume_index(path).is_some()
        && parse_global_prefix_index(file_name).is_some()
        && has_legacy_safe_name(file_name)
}

fn parse_legacy_hash8_from_name(file_name: &str) -> Option<String> {
    let lower = file_name.to_ascii_lowercase();
    if !lower.ends_with(".mp3") {
        return None;
    }

    let stem = lower.strip_suffix(".mp3")?;
    let (_, hash8) = stem.rsplit_once('_')?;
    if hash8.len() != 8 || !hash8.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    Some(hash8.to_string())
}

#[cfg(test)]
fn source_hash8(path: &Path) -> Result<String> {
    let full = compute_file_sha256(path)?;
    Ok(full
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_lowercase())
        .take(8)
        .collect())
}

fn build_target_hash_index(
    target_mount: &Path,
) -> Result<(TargetHashSet, DisplacedHashMap, TargetAudioFiles)> {
    let target_report = discover_audio_files(target_mount)?;
    let mut compliant_hashes = HashSet::new();
    let mut non_compliant_hash_to_path: HashMap<String, PathBuf> = HashMap::new();

    for file in &target_report.audio_files {
        if is_legacy_compliant_target(&file.path, &file.filename) {
            if let Some(hash8) = parse_legacy_hash8_from_name(&file.filename) {
                compliant_hashes.insert(hash8);
            }
        } else {
            let hash = compute_file_sha256(&file.path)?;
            non_compliant_hash_to_path
                .entry(hash)
                .or_insert_with(|| file.path.clone());
        }
    }

    Ok((
        compliant_hashes,
        non_compliant_hash_to_path,
        target_report.audio_files,
    ))
}

/// Accumulator for the source-side pass of a sync diff.
struct SourceDiffAcc {
    files_to_process: Vec<AudioFile>,
    skipped_existing: usize,
    displaced_in_target: HashMap<PathBuf, PathBuf>,
    displaced_planned_paths: HashSet<PathBuf>,
}

impl SourceDiffAcc {
    fn new() -> Self {
        Self {
            files_to_process: Vec::new(),
            skipped_existing: 0,
            displaced_in_target: HashMap::new(),
            displaced_planned_paths: HashSet::new(),
        }
    }

    fn process(
        &mut self,
        source: AudioFile,
        target_hashes: &TargetHashSet,
        displaced_map: &DisplacedHashMap,
    ) -> Result<()> {
        let src_hash = compute_file_sha256(&source.path)?;
        let src_hash8: String = src_hash
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_lowercase())
            .take(8)
            .collect();

        if src_hash8.len() != 8 {
            return Err(anyhow::anyhow!(
                "Invalid SHA256 for source file '{}': cannot derive hash8",
                source.path.display()
            ));
        }

        if target_hashes.contains(&src_hash8) {
            self.skipped_existing += 1;
        } else if let Some(usb_path) = displaced_map.get(&src_hash) {
            let usb_path = usb_path.clone();
            self.displaced_in_target.insert(source.path.clone(), usb_path.clone());
            self.displaced_planned_paths.insert(usb_path);
            self.files_to_process.push(source);
        } else {
            self.files_to_process.push(source);
        }
        Ok(())
    }
}

fn finish_sync_diff(
    acc: SourceDiffAcc,
    target_files: TargetAudioFiles,
    checkpoint_known_names: &HashSet<String>,
) -> Result<SyncDiffReport> {
    let SourceDiffAcc {
        files_to_process,
        skipped_existing,
        displaced_in_target,
        displaced_planned_paths,
    } = acc;

    let mut existing_volume_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut max_existing_index = 0usize;
    let mut untracked_in_target = Vec::new();

    for target in target_files {
        let is_compliant = is_legacy_compliant_target(&target.path, &target.filename);

        if is_compliant {
            let Some(vol_idx) = parse_volume_index(&target.path) else {
                untracked_in_target.push(target.path.clone());
                continue;
            };
            *existing_volume_counts.entry(vol_idx).or_insert(0) += 1;

            let Some(idx) = parse_global_prefix_index(&target.filename) else {
                untracked_in_target.push(target.path.clone());
                continue;
            };
            if idx > max_existing_index {
                max_existing_index = idx;
            }
        } else {
            // R-32: El contenido por hash no basta; si la topologia no cumple,
            // el archivo se trata como huerfano y se aísla en cuarentena.
            if !displaced_planned_paths.contains(&target.path) {
                untracked_in_target.push(target.path.clone());
            }
            continue;
        }

        if !checkpoint_known_names.is_empty() && !checkpoint_known_names.contains(&target.filename)
        {
            untracked_in_target.push(target.path.clone());
        }
    }

    let mut files_to_process = files_to_process;
    files_to_process.sort_by(|a, b| {
        let a_name = a.filename.to_ascii_lowercase();
        let b_name = b.filename.to_ascii_lowercase();
        a_name
            .cmp(&b_name)
            .then_with(|| a.path.to_string_lossy().cmp(&b.path.to_string_lossy()))
    });

    Ok(SyncDiffReport {
        files_to_process,
        skipped_existing,
        untracked_in_target,
        displaced_in_target,
        existing_volume_counts,
        max_existing_index,
    })
}

pub fn calculate_sync_diff(
    source_files: &[AudioFile],
    target_mount: &Path,
    checkpoint_known_names: &HashSet<String>,
) -> Result<SyncDiffReport> {
    let (target_hashes, displaced_hash_to_path, target_files) =
        build_target_hash_index(target_mount)?;

    let mut acc = SourceDiffAcc::new();
    for source in source_files {
        acc.process(source.clone(), &target_hashes, &displaced_hash_to_path)?;
    }

    finish_sync_diff(acc, target_files, checkpoint_known_names)
}

/// Streaming variant of [`calculate_sync_diff`].
///
/// Builds the USB target hash index first (bounded by USB file count), then
/// streams source files one at a time via [`visit_audio_files`] — avoiding
/// materialising the full source `Vec<AudioFile>` before the diff starts.
///
/// Use this path when `sync_mode` is active and `strict_parity` is not required.
pub fn calculate_sync_diff_streaming(
    source_root: &Path,
    target_mount: &Path,
    checkpoint_known_names: &HashSet<String>,
) -> Result<SyncDiffReport> {
    let (target_hashes, displaced_hash_to_path, target_files) =
        build_target_hash_index(target_mount)?;

    let mut acc = SourceDiffAcc::new();
    visit_audio_files(source_root, |source| {
        acc.process(source, &target_hashes, &displaced_hash_to_path)
    })?;

    finish_sync_diff(acc, target_files, checkpoint_known_names)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    fn legacy_name(index: usize, stem: &str, hash8: &str) -> String {
        let mut safe_stem: String = stem
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        safe_stem = safe_stem.chars().take(15).collect();
        if safe_stem.is_empty() {
            safe_stem = "audio".to_string();
        }
        while safe_stem.len() < 15 {
            safe_stem.push('_');
        }
        format!("{:03}_{}_{}.mp3", index, safe_stem, hash8)
    }

    #[test]
    fn test_calculate_sync_diff_skips_existing_by_hash() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("VOL_01"))?;

        let src_existing = source.path().join("song_a.mp3");
        let src_new = source.path().join("song_b.mp3");
        fs::write(&src_existing, b"same content")?;
        fs::write(&src_new, b"new content")?;

        let existing_hash = source_hash8(&src_existing)?;
        let usb_existing = usb
            .path()
            .join("VOL_01")
            .join(legacy_name(183, "song_a", &existing_hash));
        fs::write(&usb_existing, b"same content")?;

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &HashSet::new())?;

        assert_eq!(report.skipped_existing, 1);
        assert_eq!(report.files_to_process.len(), 1);
        assert!(report.files_to_process[0].path.ends_with("song_b.mp3"));
        assert_eq!(report.max_existing_index, 183);

        Ok(())
    }

    #[test]
    fn test_calculate_sync_diff_streaming_matches_eager() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("VOL_01"))?;

        let src_existing = source.path().join("existing.mp3");
        let src_new = source.path().join("new.mp3");
        fs::write(&src_existing, b"same content")?;
        fs::write(&src_new, b"new content")?;
        let hash = source_hash8(&src_existing)?;
        fs::write(
            usb.path()
                .join("VOL_01")
                .join(legacy_name(40, "existing", &hash)),
            b"same content",
        )?;

        let known_names = HashSet::new();
        let eager_source = discover_audio_files(source.path())?.audio_files;
        let eager = calculate_sync_diff(&eager_source, usb.path(), &known_names)?;
        let streaming = calculate_sync_diff_streaming(source.path(), usb.path(), &known_names)?;

        assert_eq!(eager.skipped_existing, streaming.skipped_existing);
        assert_eq!(eager.max_existing_index, streaming.max_existing_index);
        assert_eq!(eager.files_to_process.len(), streaming.files_to_process.len());
        assert_eq!(
            eager.untracked_in_target.len(),
            streaming.untracked_in_target.len()
        );

        Ok(())
    }

    #[test]
    fn test_calculate_sync_diff_reprocesses_hash_when_usb_topology_is_invalid() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("musica"))?;

        let src_existing = source.path().join("song_a.mp3");
        let usb_dirty = usb.path().join("musica/song_a_original.mp3");

        fs::write(&src_existing, b"same content")?;
        fs::write(&usb_dirty, b"same content")?;

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &HashSet::new())?;

        assert_eq!(report.skipped_existing, 0);
        assert_eq!(report.files_to_process.len(), 1);
        assert_eq!(report.untracked_in_target.len(), 0);
        assert_eq!(report.displaced_in_target.len(), 1);
        assert!(report
            .displaced_in_target
            .get(&src_existing)
            .expect("missing displaced mapping")
            .ends_with("musica/song_a_original.mp3"));
        assert_eq!(report.max_existing_index, 0);
        assert!(report.existing_volume_counts.is_empty());

        Ok(())
    }

    #[test]
    fn test_calculate_sync_diff_non_compliant_hash_is_displaced_not_untracked() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("musica"))?;

        let src_existing = source.path().join("song_a.mp3");
        let usb_dirty = usb.path().join("musica/song_a_original.mp3");

        fs::write(&src_existing, b"same content")?;
        fs::write(&usb_dirty, b"same content")?;

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &HashSet::new())?;

        assert_eq!(report.files_to_process.len(), 1);
        assert_eq!(report.displaced_in_target.len(), 1);
        assert_eq!(report.untracked_in_target.len(), 0);

        Ok(())
    }

    #[test]
    fn test_calculate_sync_diff_idempotent_when_everything_exists() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("VOL_01"))?;

        let src_a = source.path().join("a.mp3");
        let src_b = source.path().join("b.mp3");
        fs::write(&src_a, b"same-a")?;
        fs::write(&src_b, b"same-b")?;

        let hash_a = source_hash8(&src_a)?;
        let hash_b = source_hash8(&src_b)?;
        fs::write(
            usb.path().join("VOL_01").join(legacy_name(1, "a", &hash_a)),
            b"same-a",
        )?;
        fs::write(
            usb.path().join("VOL_01").join(legacy_name(2, "b", &hash_b)),
            b"same-b",
        )?;

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &HashSet::new())?;

        assert_eq!(report.files_to_process.len(), 0);
        assert_eq!(report.skipped_existing, 2);
        assert_eq!(report.max_existing_index, 2);
        assert!(report.untracked_in_target.is_empty());

        Ok(())
    }

    #[test]
    fn test_calculate_sync_diff_sorts_new_files_alphabetically() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        let src_z = source.path().join("zeta.mp3");
        let src_a = source.path().join("alfa.mp3");
        let src_m = source.path().join("medio.mp3");

        fs::write(&src_z, b"z")?;
        fs::write(&src_a, b"a")?;
        fs::write(&src_m, b"m")?;

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &HashSet::new())?;

        let ordered: Vec<String> = report
            .files_to_process
            .iter()
            .map(|f| f.filename.clone())
            .collect();
        assert_eq!(ordered, vec!["alfa.mp3", "medio.mp3", "zeta.mp3"]);
        Ok(())
    }

    #[test]
    fn test_calculate_sync_diff_mixed_existing_displaced_and_new() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("VOL_01"))?;
        fs::create_dir_all(usb.path().join("misc"))?;

        let src_existing = source.path().join("existing.mp3");
        let src_displaced = source.path().join("displaced.mp3");
        let src_new = source.path().join("new.mp3");

        fs::write(&src_existing, b"content-existing")?;
        fs::write(&src_displaced, b"content-displaced")?;
        fs::write(&src_new, b"content-new")?;

        let existing_hash = source_hash8(&src_existing)?;
        fs::write(
            usb.path()
                .join("VOL_01")
                .join(legacy_name(7, "existing", &existing_hash)),
            b"content-existing",
        )?;
        fs::write(
            usb.path().join("misc/displaced_original.mp3"),
            b"content-displaced",
        )?;

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &HashSet::new())?;

        assert_eq!(report.skipped_existing, 1);
        assert_eq!(report.files_to_process.len(), 2);
        assert_eq!(report.displaced_in_target.len(), 1);
        assert!(report.displaced_in_target.contains_key(&src_displaced));
        assert_eq!(report.max_existing_index, 7);

        Ok(())
    }

    #[test]
    fn test_calculate_sync_diff_marks_unknown_checkpoint_names_as_untracked() -> Result<()> {
        let source = TempDir::new()?;
        let usb = TempDir::new()?;

        fs::create_dir_all(usb.path().join("VOL_01"))?;

        let src = source.path().join("incoming.mp3");
        fs::write(&src, b"incoming-data")?;
        fs::write(usb.path().join("VOL_01/001_known.mp3"), b"known-data")?;
        fs::write(usb.path().join("VOL_01/002_orphan.mp3"), b"orphan-data")?;

        let mut known_names = HashSet::new();
        known_names.insert("001_known.mp3".to_string());

        let source_files = discover_audio_files(source.path())?.audio_files;
        let report = calculate_sync_diff(&source_files, usb.path(), &known_names)?;

        assert_eq!(report.files_to_process.len(), 1);
        assert_eq!(report.skipped_existing, 0);
        assert_eq!(report.untracked_in_target.len(), 1);
        assert!(report.untracked_in_target[0].ends_with("VOL_01/002_orphan.mp3"));

        Ok(())
    }

    #[test]
    fn test_quarantine_untracked_files_backup_first() -> Result<()> {
        let usb = TempDir::new()?;
        let backup_root = TempDir::new()?;

        let orphan = usb.path().join("VOL_01").join("manual_song.mp3");
        fs::create_dir_all(orphan.parent().unwrap())?;
        fs::write(&orphan, b"orphan-data")?;

        let mut backup_meta = BackupMetadata::new_with_base_dir(Some(backup_root.path()))?;
        let report = quarantine_untracked_files(
            usb.path(),
            std::slice::from_ref(&orphan),
            &mut backup_meta,
            "sync_test",
        )?;

        assert_eq!(report.failed.len(), 0);
        assert_eq!(report.quarantined.len(), 1);
        assert!(!orphan.exists());
        assert!(report.quarantined[0].exists());
        assert!(backup_meta.file_count >= 1);

        Ok(())
    }

    #[test]
    fn test_analyze_root_topology_preserves_unmanaged_user_content() -> Result<()> {
        let usb = TempDir::new()?;
        fs::create_dir_all(usb.path().join("musica_cliente"))?;
        fs::write(usb.path().join("nota.txt"), b"cliente")?;

        let report = analyze_root_topology(usb.path())?;

        assert_eq!(report.policy, RootContentPolicy::PreserveUserContent);
        assert_eq!(report.non_whitelisted_entries.len(), 2);
        Ok(())
    }

    #[test]
    fn test_analyze_root_topology_detects_managed_usb() -> Result<()> {
        let usb = TempDir::new()?;
        fs::create_dir_all(usb.path().join("VOL_01"))?;
        fs::create_dir_all(usb.path().join("legacy_dump"))?;

        let report = analyze_root_topology(usb.path())?;

        assert_eq!(report.policy, RootContentPolicy::ManagedTopology);
        assert_eq!(report.non_whitelisted_entries.len(), 1);
        assert!(report.non_whitelisted_entries[0].ends_with("legacy_dump"));
        Ok(())
    }

    #[test]
    fn test_analyze_root_topology_empty_usb() -> Result<()> {
        let usb = TempDir::new()?;

        let report = analyze_root_topology(usb.path())?;

        assert_eq!(report.policy, RootContentPolicy::Empty);
        assert!(report.non_whitelisted_entries.is_empty());
        Ok(())
    }

    #[test]
    fn test_quarantine_non_whitelisted_root_entries_backup_first() -> Result<()> {
        let usb = TempDir::new()?;
        let backup_root = TempDir::new()?;

        fs::create_dir_all(usb.path().join("VOL_01"))?;
        fs::create_dir_all(usb.path().join("legacy_dump"))?;
        fs::write(usb.path().join("legacy_dump/song.mp3"), b"legacy-audio")?;
        fs::write(usb.path().join("documento.pdf"), b"pdf-data")?;

        let entries = collect_non_whitelisted_root_entries(usb.path())?;
        assert_eq!(entries.len(), 2);

        let mut backup_meta = BackupMetadata::new_with_base_dir(Some(backup_root.path()))?;
        let report = quarantine_non_whitelisted_root_entries(
            usb.path(),
            &entries,
            &mut backup_meta,
            "topology_test",
        )?;

        assert!(report.failed.is_empty());
        assert_eq!(report.quarantined.len(), 2);
        assert!(!usb.path().join("legacy_dump").exists());
        assert!(!usb.path().join("documento.pdf").exists());
        assert!(usb
            .path()
            .join(".legacy_quarantine")
            .join("topology_test")
            .exists());
        assert!(backup_meta.file_count >= 2);

        Ok(())
    }
}
