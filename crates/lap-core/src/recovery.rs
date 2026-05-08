//! R-17: Rollback y Disaster Recovery
//!
//! Implementa recuperación ante fallos consumiendo el CheckpointData:
//! - Verificación criptográfica (SHA256) de la integridad de la USB.
//! - Detección de archivos corruptos (ej. 0 bytes por fallo FAT32).
//! - Restauración granular (solo recopia lo que está roto o falta).

use crate::checkpoint::{CheckpointData, CheckpointManager, OperationStatus};
use crate::crypto::compute_file_sha256;
use crate::normalizer;
use crate::security::validate_path_containment;
use anyhow::{anyhow, Context, Result};
use log::{error, info, warn};
use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum IntegrityStatus {
    Ok,
    Incomplete,
    Corrupted(Vec<PathBuf>), // Contiene la lista de archivos corruptos/faltantes
}

pub struct RecoveryManager {
    backup_dir: PathBuf,
    usb_mount: PathBuf,
}

impl RecoveryManager {
    pub fn new(backup_dir: PathBuf, usb_mount: PathBuf) -> Self {
        RecoveryManager {
            backup_dir,
            usb_mount,
        }
    }

    fn is_valid_sha256_hex(hash: &str) -> bool {
        hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
    }

    fn target_path_for(index: usize, normalized_name: &str, usb_mount: &Path) -> Result<PathBuf> {
        let volume_index = (index / 50) + 1;
        let volume_folder = format!("VOL_{:02}", volume_index);

        let vol_path = validate_path_containment(usb_mount, Path::new(&volume_folder))?;
        validate_path_containment(&vol_path, Path::new(normalized_name))
    }

    fn force_mp3_name(name: &str) -> String {
        let path = Path::new(name);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.to_string());
        format!("{}.mp3", stem)
    }

    fn purge_orphan_zero_byte_files(&self, checkpoint: &CheckpointData) -> Result<usize> {
        let expected_paths: HashSet<PathBuf> = checkpoint
            .processed_files
            .iter()
            .filter_map(|(idx, f)| {
                Self::target_path_for(*idx, &f.normalized_name, &self.usb_mount).ok()
            })
            .collect();

        if !self.usb_mount.exists() {
            return Ok(0);
        }

        let mut purged = 0usize;

        for volume_entry in fs::read_dir(&self.usb_mount)? {
            let volume_entry = volume_entry?;
            if !volume_entry.file_type()?.is_dir() {
                continue;
            }

            let volume_name = volume_entry.file_name().to_string_lossy().to_string();
            if !volume_name.starts_with("VOL_") {
                continue;
            }

            for file_entry in fs::read_dir(volume_entry.path())? {
                let file_entry = file_entry?;
                if !file_entry.file_type()?.is_file() {
                    continue;
                }

                let path = file_entry.path();
                let size = file_entry.metadata()?.len();
                if size == 0 && !expected_paths.contains(&path) {
                    let _ = fs::remove_file(&path);
                    purged += 1;
                }
            }
        }

        Ok(purged)
    }

    /// [R-09-008] Granular Recovery
    /// Legacy cross-ref: R-17.
    /// Precondición: los datos del checkpoint reflejan el estado de la última sesión confiable y la ruta de montaje USB es el destino pretendido.
    /// Postcondición: retorna estado de integridad basado en presencia física de archivos y acuerdo SHA256.
    /// Invariante: no se puede confiar en un archivo marcado como `Completed` en el checkpoint sin revalidación contra estado en disco.
    ///
    /// Verifica la integridad de la USB basándose en el Checkpoint
    pub fn verify_usb_integrity(&self, checkpoint: &CheckpointData) -> Result<IntegrityStatus> {
        info!("Verifying USB integrity against checkpoint...");

        if !self.usb_mount.exists() {
            warn!("USB mount point is completely inaccessible.");
            return Ok(IntegrityStatus::Incomplete);
        }

        let mut corrupted_files = Vec::new();

        for (index, file_data) in &checkpoint.processed_files {
            // Solo verificamos archivos que el checkpoint afirma haber completado
            if file_data.status == OperationStatus::Completed {
                // Inferir la ruta destino.
                // Sabemos que max_capacity era 50. Calculamos a qué volumen pertenece.
                let target_path = match Self::target_path_for(
                    *index,
                    &file_data.normalized_name,
                    &self.usb_mount,
                ) {
                    Ok(path) => path,
                    Err(e) => {
                        warn!("R-05 Path containment failure on checkpoint entry: {}", e);
                        continue;
                    }
                };

                if !target_path.exists() {
                    warn!("Missing file detected: {}", target_path.display());
                    corrupted_files.push(target_path);
                    continue;
                }

                // Si el archivo existe, pero el checkpoint tiene un hash esperado, lo verificamos.
                if let Some(expected_hash) = &file_data.usb_checksum {
                    if !Self::is_valid_sha256_hex(expected_hash) {
                        warn!(
                            "Invalid checkpoint hash detected on {}",
                            target_path.display()
                        );
                        corrupted_files.push(target_path);
                        continue;
                    }
                    match compute_file_sha256(&target_path) {
                        Ok(actual_hash) => {
                            if actual_hash != *expected_hash {
                                warn!(
                                    "Corruption detected (Hash mismatch): {}",
                                    target_path.display()
                                );
                                corrupted_files.push(target_path);
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Corruption detected (Unreadable file): {} - {}",
                                target_path.display(),
                                e
                            );
                            corrupted_files.push(target_path);
                        }
                    }
                } else {
                    warn!(
                        "Missing checkpoint hash detected on {}",
                        target_path.display()
                    );
                    corrupted_files.push(target_path);
                }
            }
        }

        if corrupted_files.is_empty() {
            if checkpoint.operation_status == OperationStatus::Completed {
                info!("✓ USB Integrity OK. All checksums match.");
                Ok(IntegrityStatus::Ok)
            } else {
                info!("USB is partially written but not corrupted.");
                Ok(IntegrityStatus::Incomplete)
            }
        } else {
            error!(
                "❌ Found {} corrupted or missing files on USB.",
                corrupted_files.len()
            );
            Ok(IntegrityStatus::Corrupted(corrupted_files))
        }
    }

    /// [R-09-008] Granular Recovery
    /// Legacy cross-ref: R-17.
    /// Referencia legacy: R-17.
    /// Precondición: el gestor de checkpoint fue cargado desde una sesión recuperable y los archivos de origen permanecen disponibles.
    /// Postcondición: solo se retentan entradas faltantes, corruptas, incompletas o con hash inválido.
    /// Invariante: recuperación es selectiva e idempotente; no debe recopiar archivos ya válidos ni reprocesar entradas DRM-omitidas.
    ///
    /// Restaura los archivos corruptos copiándolos de nuevo desde el origen
    pub fn execute_recovery(&self, checkpoint_mgr: &mut CheckpointManager) -> Result<()> {
        let checkpoint_data = checkpoint_mgr.get_data().clone();

        info!("Starting granular recovery process...");
        info!(
            "Recovery context backup directory: {}",
            self.backup_dir.display()
        );
        let integrity = self.verify_usb_integrity(&checkpoint_data)?;

        let mut indices_to_recover = BTreeSet::new();

        // Reintento de entradas no finalizadas correctamente.
        for (index, file_data) in &checkpoint_data.processed_files {
            if file_data.error_message.as_deref() == Some("Skipped_DRM") {
                // Entrada cuarentenada por DRM: no es recuperable por diseño.
                continue;
            }

            if file_data.status != OperationStatus::Completed {
                indices_to_recover.insert(*index);
            } else {
                let has_invalid_hash = file_data
                    .usb_checksum
                    .as_ref()
                    .map(|h| !Self::is_valid_sha256_hex(h))
                    .unwrap_or(true);
                if has_invalid_hash || file_data.error_message.is_some() {
                    indices_to_recover.insert(*index);
                }
            }
        }

        let checkpoint_data = checkpoint_mgr.get_data().clone();

        if let IntegrityStatus::Corrupted(bad_files) = integrity {
            for bad_file_path in bad_files {
                if let Some(file_name) = bad_file_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                {
                    if let Some((index, _)) = checkpoint_data
                        .processed_files
                        .iter()
                        .find(|(_, f)| f.normalized_name == file_name)
                    {
                        indices_to_recover.insert(*index);
                    }
                }
            }
        }

        if indices_to_recover.is_empty() {
            let purged = self.purge_orphan_zero_byte_files(&checkpoint_data)?;
            if purged > 0 {
                info!("Purged {} orphan zero-byte file(s) from USB.", purged);
            }
            info!("No corrupted files to recover.");
            return Ok(());
        }

        info!(
            "Attempting to recover {} files...",
            indices_to_recover.len()
        );
        let mut recovery_failures = Vec::new();

        for index in indices_to_recover {
            let Some(file_data) = checkpoint_data.processed_files.get(&index) else {
                recovery_failures.push(format!("Index {} missing in checkpoint", index));
                continue;
            };

            // Compatibilidad con checkpoints antiguos: migrar destino a .mp3 para evitar
            // incompatibilidad codec/contenedor en normalizacion.
            let effective_name = if file_data
                .normalized_name
                .to_ascii_lowercase()
                .ends_with(".mp3")
            {
                file_data.normalized_name.clone()
            } else {
                let migrated = Self::force_mp3_name(&file_data.normalized_name);
                checkpoint_mgr.update_file_normalized_name(index, migrated.clone())?;

                // Purga artefactos residuales en la ruta antigua (ej. .flac de 0 bytes).
                let legacy_path =
                    Self::target_path_for(index, &file_data.normalized_name, &self.usb_mount)?;
                if legacy_path.exists() {
                    let _ = fs::remove_file(&legacy_path);
                }

                migrated
            };

            let bad_file_path = Self::target_path_for(index, &effective_name, &self.usb_mount)?;

            // Eliminar el archivo malo si existe (para evitar bloqueos FAT32)
            if bad_file_path.exists() {
                let _ = fs::remove_file(&bad_file_path);
            }

            if let Some(parent) = bad_file_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Failed to create recovery target directory: {}",
                        parent.display()
                    )
                })?;
            }

            info!(
                "Recovering '{}' from '{}'",
                effective_name,
                file_data.original_path.display()
            );

            let processing_decision = normalizer::classify_audio_processing(&file_data.original_path)?;

            let recovery_result = match processing_decision {
                normalizer::ProcessingDecision::FastInPlaceRename => {
                    fs::copy(&file_data.original_path, &bad_file_path)
                        .map(|_| ())
                        .with_context(|| {
                            format!(
                                "Recovery copy failed for '{}'",
                                file_data.original_path.display()
                            )
                        })
                }
                normalizer::ProcessingDecision::FfmpegCopyClean
                | normalizer::ProcessingDecision::FfmpegTranscode => {
                    normalizer::normalize_audio(
                        &file_data.original_path,
                        &bad_file_path,
                        processing_decision,
                    )
                }
            };

            match recovery_result {
                Ok(_) => {
                    let usb_hash = compute_file_sha256(&bad_file_path)?;
                    checkpoint_mgr.mark_file_completed(index, usb_hash)?;

                    // Forzar sync FAT32
                    if let Some(parent) = bad_file_path.parent() {
                        if let Ok(dir) = File::open(parent) {
                            let _ = dir.sync_all();
                        }
                    }
                }
                Err(e) => {
                    let err_msg =
                        format!("Recovery normalization failed for index {}: {}", index, e);
                    checkpoint_mgr.mark_file_failed(index, err_msg.clone())?;
                    recovery_failures.push(err_msg);
                }
            }
        }

        if recovery_failures.is_empty() {
            let latest_checkpoint = checkpoint_mgr.get_data().clone();
            let purged = self.purge_orphan_zero_byte_files(&latest_checkpoint)?;
            if purged > 0 {
                info!("Purged {} orphan zero-byte file(s) from USB.", purged);
            }
            info!("✓ Granular recovery completed successfully.");
        } else {
            return Err(anyhow!(
                "Recovery completed with {} failure(s): {}",
                recovery_failures.len(),
                recovery_failures.join(" | ")
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::FileCheckpoint;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn test_integrity_verification_missing_file() -> Result<()> {
        let temp_usb = TempDir::new()?;
        let temp_backup = TempDir::new()?;

        let mut checkpoint = CheckpointData::new(
            temp_backup.path().to_path_buf(),
            temp_usb.path().to_path_buf(),
            PathBuf::from("/fake/audio"),
            1,
        );

        let file_data = FileCheckpoint {
            original_path: PathBuf::from("/fake/source.mp3"),
            normalized_name: "001_test.mp3".to_string(),
            status: OperationStatus::Completed,
            original_checksum: "hash".to_string(),
            usb_checksum: Some("hash".to_string()),
            start_time: Utc::now(),
            end_time: Some(Utc::now()),
            error_message: None,
        };

        checkpoint.processed_files.insert(0, file_data);
        checkpoint.operation_status = OperationStatus::Completed;

        let recovery = RecoveryManager::new(
            temp_backup.path().to_path_buf(),
            temp_usb.path().to_path_buf(),
        );

        // El archivo "VOL_01/001_test.mp3" no existe en temp_usb
        let status = recovery.verify_usb_integrity(&checkpoint)?;

        match status {
            IntegrityStatus::Corrupted(files) => {
                assert_eq!(files.len(), 1);
                assert!(files[0].ends_with("VOL_01/001_test.mp3"));
            }
            _ => panic!("Expected corrupted status due to missing file"),
        }

        Ok(())
    }
}
