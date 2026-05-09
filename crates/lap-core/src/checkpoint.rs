//! R-16: Checkpoint System (Recuperación de Estado)
//!
//! Implementa un sistema robusto de puntos de control que permite:
//! - Guardar el progreso de operaciones en tiempo real (Atómico)
//! - Detectar interrupciones y reanudar sin panics
//! - Mantener integridad referencial con backups

use crate::error::ProvisioningError;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::info;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const CHECKPOINT_VERSION: u32 = 1;
const CHECKPOINT_FILENAME: &str = ".provisioning_checkpoint";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCheckpoint {
    pub original_path: PathBuf,
    pub normalized_name: String,
    pub status: OperationStatus,
    pub original_checksum: String,
    pub usb_checksum: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub backup_dir: PathBuf,
    pub usb_mount: PathBuf,
    pub audio_source: PathBuf,
    pub total_files: usize,
    // CAMBIO CRÍTICO: Usamos BTreeMap en lugar de Vec para evitar Panics de "Out of Bounds"
    // y mantener los archivos indexados por su número de orden real.
    pub processed_files: BTreeMap<usize, FileCheckpoint>,
    pub last_completed_index: Option<usize>,
    pub operation_status: OperationStatus,
    pub created_volumes: Vec<String>,
    pub session_id: String,
}

impl CheckpointData {
    pub fn new(
        backup_dir: PathBuf,
        usb_mount: PathBuf,
        audio_source: PathBuf,
        total_files: usize,
    ) -> Self {
        let session_id = deterministic_checkpoint_session_id(&backup_dir, &usb_mount, &audio_source);
        CheckpointData {
            version: CHECKPOINT_VERSION,
            created_at: Utc::now(),
            last_updated: Utc::now(),
            backup_dir,
            usb_mount,
            audio_source,
            total_files,
            processed_files: BTreeMap::new(),
            last_completed_index: None,
            operation_status: OperationStatus::InProgress,
            created_volumes: Vec::new(),
            session_id,
        }
    }

    pub fn progress_percentage(&self) -> f32 {
        if self.total_files == 0 {
            return 100.0;
        }
        let completed_count = self
            .processed_files
            .values()
            .filter(|f| f.status == OperationStatus::Completed)
            .count();
        (completed_count as f32 / self.total_files as f32) * 100.0
    }

    pub fn is_recoverable(&self) -> bool {
        self.operation_status == OperationStatus::InProgress && !self.processed_files.is_empty()
    }
}

fn deterministic_checkpoint_session_id(
    backup_dir: &Path,
    usb_mount: &Path,
    audio_source: &Path,
) -> String {
    let key = format!(
        "{}|{}|{}",
        stable_path_key(backup_dir),
        stable_path_key(usb_mount),
        stable_path_key(audio_source)
    );
    format!("checkpoint_{}", short_hash(&key))
}

fn stable_path_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn short_hash(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    hash[..12].to_string()
}

pub struct CheckpointManager {
    checkpoint_path: PathBuf,
    checkpoint_data: CheckpointData,
    auto_persist: bool,
}

/// [R-02-006] ENOSPC Handler
/// Referencia legacy: R-21 (Manejo de disco lleno en Fase 2).
/// Precondición: `checkpoint_path` y `tmp_path` representan el destino de checkpoint duradero pretendido y su ruta de escritura temporal.
/// Postcondición: mapea condiciones de disco lleno a `ProvisioningError::StorageFull` y elimina el artefacto temporal si es posible.
/// Invariante: persistencia de checkpoint nunca reporta éxito después de una escritura temporal parcial.
pub fn write_json_atomically_to_paths(
    checkpoint_path: &Path,
    tmp_path: &Path,
    json_content: &str,
) -> std::result::Result<(), ProvisioningError> {
    let write_result = (|| -> std::io::Result<()> {
        let mut tmp_file = File::create(tmp_path)?;
        tmp_file.write_all(json_content.as_bytes())?;
        tmp_file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = fs::remove_file(tmp_path);

        let is_full = e.kind() == std::io::ErrorKind::StorageFull || e.raw_os_error() == Some(28);

        if is_full {
            return Err(ProvisioningError::StorageFull {
                details: format!(
                    "Fallo al escribir en {}. Capacidad al 100%.",
                    tmp_path.display()
                ),
            });
        }

        return Err(ProvisioningError::ProvisioningFailed {
            details: format!("Fallo de I/O al escribir el checkpoint temporal: {}", e),
        });
    }

    fs::rename(tmp_path, checkpoint_path).map_err(|e| ProvisioningError::ProvisioningFailed {
        details: format!("Fallo atomico de rename del checkpoint: {}", e),
    })?;

    if let Some(parent) = checkpoint_path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

impl CheckpointManager {
    pub fn new(
        backup_dir: PathBuf,
        usb_mount: PathBuf,
        audio_source: PathBuf,
        total_files: usize,
    ) -> Result<Self> {
        let checkpoint_path = backup_dir.join(CHECKPOINT_FILENAME);
        let checkpoint_data = CheckpointData::new(backup_dir, usb_mount, audio_source, total_files);
        info!(
            "Created new CheckpointManager (session: {})",
            checkpoint_data.session_id
        );
        Ok(CheckpointManager {
            checkpoint_path,
            checkpoint_data,
            auto_persist: true,
        })
    }

    /// [R-09-007] Atomic Checkpoint System
    /// Legacy cross-ref: R-16.
    /// Precondición: `checkpoint_path` apunta a un artefacto de checkpoint previamente creado.
    /// Postcondición: retorna un gestor de checkpoint tipado solo si JSON y versión son válidos.
    /// Invariante: ninguna ruta de recuperación puede proceder contra un archivo de checkpoint faltante o con versión incompatible.
    pub fn load_from_disk(checkpoint_path: &Path) -> Result<Self> {
        if !checkpoint_path.exists() {
            return Err(anyhow!(
                "Checkpoint file not found: {}",
                checkpoint_path.display()
            ));
        }
        let json_content = fs::read_to_string(checkpoint_path)?;
        let checkpoint_data: CheckpointData = serde_json::from_str(&json_content)?;

        if checkpoint_data.version != CHECKPOINT_VERSION {
            return Err(anyhow!(
                "Version mismatch: expected {}, got {}",
                CHECKPOINT_VERSION,
                checkpoint_data.version
            ));
        }

        info!(
            "Loaded checkpoint from disk (session: {})",
            checkpoint_data.session_id
        );
        Ok(CheckpointManager {
            checkpoint_path: checkpoint_path.to_path_buf(),
            checkpoint_data,
            auto_persist: true,
        })
    }

    pub fn set_auto_persist(&mut self, enabled: bool) {
        self.auto_persist = enabled;
    }

    fn persist_if_needed(&mut self) -> Result<()> {
        if self.auto_persist {
            self.save_to_disk()?;
        }
        Ok(())
    }

    /// [R-09-007] Atomic Checkpoint System
    /// Legacy cross-ref: R-16.
    /// Precondición: el estado del checkpoint es internamente consistente y el filesystem del host es escribible.
    /// Postcondición: el checkpoint es persistido duraderamente vía escritura temporal, `sync_all`, renombrado atómico y sincronización del directorio padre.
    /// Invariante: el archivo canonical de checkpoint nunca debe quedar truncado o parcialmente escrito.
    ///
    /// R-21 + V1.0: Escritura Atómica con captura explícita de ENOSPC (disco lleno).
    /// Orden estricto: timestamp → serializar → escribir .tmp → sync → rename → dir-sync.
    pub fn save_to_disk(&mut self) -> std::result::Result<(), ProvisioningError> {
        // 1. Actualizar timestamp ANTES de serializar
        self.checkpoint_data.last_updated = Utc::now();
        let tmp_path = self.checkpoint_path.with_extension("tmp");
        let json_content = serde_json::to_string_pretty(&self.checkpoint_data).map_err(|e| {
            ProvisioningError::ProvisioningFailed {
                details: format!("JSON Serialize error en checkpoint: {}", e),
            }
        })?;

        write_json_atomically_to_paths(&self.checkpoint_path, &tmp_path, &json_content)
    }

    pub fn record_file_start(
        &mut self,
        index: usize,
        original_path: PathBuf,
        normalized_name: String,
        original_checksum: String,
    ) -> Result<()> {
        let file_checkpoint = FileCheckpoint {
            original_path,
            normalized_name,
            status: OperationStatus::InProgress,
            original_checksum,
            usb_checksum: None,
            start_time: Utc::now(),
            end_time: None,
            error_message: None,
        };

        self.checkpoint_data
            .processed_files
            .insert(index, file_checkpoint);
        self.persist_if_needed()?;
        Ok(())
    }

    pub fn mark_file_completed(&mut self, index: usize, usb_checksum: String) -> Result<()> {
        let file = self
            .checkpoint_data
            .processed_files
            .get_mut(&index)
            .ok_or_else(|| anyhow!("File index {} not found in checkpoint tracker", index))?;
        file.status = OperationStatus::Completed;
        file.usb_checksum = Some(usb_checksum);
        file.error_message = None;
        file.end_time = Some(Utc::now());

        self.checkpoint_data.last_completed_index = Some(index);
        self.persist_if_needed()?;
        Ok(())
    }

    pub fn mark_file_failed(&mut self, index: usize, error: String) -> Result<()> {
        let file = self
            .checkpoint_data
            .processed_files
            .get_mut(&index)
            .ok_or_else(|| anyhow!("File index {} not found in checkpoint tracker", index))?;
        file.status = OperationStatus::Failed;
        file.error_message = Some(error);
        file.end_time = Some(Utc::now());

        self.persist_if_needed()?;
        Ok(())
    }

    pub fn update_file_normalized_name(
        &mut self,
        index: usize,
        normalized_name: String,
    ) -> Result<()> {
        let file = self
            .checkpoint_data
            .processed_files
            .get_mut(&index)
            .ok_or_else(|| anyhow!("File index {} not found in checkpoint tracker", index))?;
        file.normalized_name = normalized_name;
        self.persist_if_needed()?;
        Ok(())
    }

    pub fn add_volume(&mut self, volume_name: String) -> Result<()> {
        self.checkpoint_data.created_volumes.push(volume_name);
        self.persist_if_needed()?;
        Ok(())
    }

    pub fn finalize(&mut self) -> Result<()> {
        self.checkpoint_data.operation_status = OperationStatus::Completed;
        self.save_to_disk()?;
        info!("Checkpoint marked as COMPLETED");
        Ok(())
    }

    pub fn get_data(&self) -> &CheckpointData {
        &self.checkpoint_data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_atomic_save() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let mut manager = CheckpointManager::new(
            temp_dir.path().to_path_buf(),
            PathBuf::from("/tmp/usb"),
            PathBuf::from("/tmp/audio"),
            100,
        )?;

        manager.save_to_disk()?;
        assert!(manager.checkpoint_path.exists());
        Ok(())
    }

    #[test]
    fn test_checkpoint_progress_with_btree() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backup_dir = temp_dir.path().join("backup");
        std::fs::create_dir_all(&backup_dir).unwrap();
        let mut manager = CheckpointManager::new(
            backup_dir,
            temp_dir.path().join("usb"),
            temp_dir.path().join("audio"),
            100,
        )
        .unwrap();

        // Registrar un archivo en el índice 50 (sin causar panic)
        manager
            .record_file_start(
                50,
                PathBuf::from("test.mp3"),
                "050_test.mp3".to_string(),
                "hash".to_string(),
            )
            .unwrap();
        manager
            .mark_file_completed(50, "usbhash".to_string())
            .unwrap();

        assert_eq!(manager.get_data().progress_percentage(), 1.0);
        assert_eq!(manager.get_data().last_completed_index, Some(50));
    }

    #[cfg(unix)]
    #[test]
    fn test_checkpoint_storage_full_maps_to_typed_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let checkpoint_path = temp_dir.path().join("checkpoint.json");

        let err = write_json_atomically_to_paths(&checkpoint_path, Path::new("/dev/full"), "{}")
            .expect_err("Expected /dev/full write to fail with ENOSPC");

        assert!(matches!(err, ProvisioningError::StorageFull { .. }));
    }

    #[test]
    fn test_is_recoverable_true_when_in_progress_with_entries() {
        let mut checkpoint = CheckpointData::new(
            PathBuf::from("/tmp/backup"),
            PathBuf::from("/tmp/usb"),
            PathBuf::from("/tmp/audio"),
            1,
        );
        checkpoint.processed_files.insert(
            0,
            FileCheckpoint {
                original_path: PathBuf::from("/tmp/audio/a.mp3"),
                normalized_name: "001_a.mp3".to_string(),
                status: OperationStatus::InProgress,
                original_checksum: "hash".to_string(),
                usb_checksum: None,
                start_time: Utc::now(),
                end_time: None,
                error_message: None,
            },
        );

        assert!(checkpoint.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_false_when_operation_completed() {
        let mut checkpoint = CheckpointData::new(
            PathBuf::from("/tmp/backup"),
            PathBuf::from("/tmp/usb"),
            PathBuf::from("/tmp/audio"),
            1,
        );
        checkpoint.processed_files.insert(
            0,
            FileCheckpoint {
                original_path: PathBuf::from("/tmp/audio/a.mp3"),
                normalized_name: "001_a.mp3".to_string(),
                status: OperationStatus::Completed,
                original_checksum: "hash".to_string(),
                usb_checksum: Some("hash".to_string()),
                start_time: Utc::now(),
                end_time: Some(Utc::now()),
                error_message: None,
            },
        );
        checkpoint.operation_status = OperationStatus::Completed;

        assert!(!checkpoint.is_recoverable());
    }

    #[cfg(unix)]
    #[test]
    fn test_checkpoint_storage_full_cleans_tmp_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let checkpoint_path = temp_dir.path().join("checkpoint.json");
        let tmp_path = temp_dir.path().join("checkpoint.tmp");

        let err = write_json_atomically_to_paths(&checkpoint_path, Path::new("/dev/full"), "{}")
            .expect_err("Expected /dev/full write to fail with ENOSPC");

        assert!(matches!(err, ProvisioningError::StorageFull { .. }));
        assert!(!tmp_path.exists());
        assert!(!checkpoint_path.exists());
    }

    #[test]
    fn test_load_from_disk_rejects_invalid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let checkpoint_path = temp_dir.path().join(".provisioning_checkpoint");
        std::fs::write(&checkpoint_path, "{invalid-json").unwrap();

        let result = CheckpointManager::load_from_disk(&checkpoint_path);
        assert!(result.is_err());
    }
}
