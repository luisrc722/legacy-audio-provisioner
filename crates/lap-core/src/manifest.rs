//! R-05-002: Catálogo de Contenido Procesado
//!
//! Mantiene un registro persistente en la USB de todas las canciones que completaron
//! el pipeline de provisión:
//! - Nombre final determinístico (con índice + hash)
//! - SHA256 completo del contenido
//! - Timestamp de procesamiento
//!
//! Usado por --sync futuro para:
//! - Deduplicación rápida (mismo hash = ya existe)
//! - Evitar reprocessamiento de duplicados
//! - Auditoría de qué se procesó cuándo

use anyhow::Result;
use crate::crypto::compute_file_sha256;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

const MANIFEST_FILENAME: &str = ".provisioning_manifest";
const MANIFEST_VERSION: u32 = 1;

/// Entrada de una canción ya procesada en el manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedFileEntry {
    /// Nombre final determinístico con la que se guardó en USB
    pub final_name: String,

    /// SHA256 completo del contenido (para dedupe exacto)
    pub content_hash: String,

    /// Tamaño en bytes
    pub size_bytes: u64,

    /// Ruta relativa en la USB donde está guardada
    pub usb_relative_path: String,

    /// Cuándo se procesó
    pub processed_at: DateTime<Utc>,

    /// Índice global secuencial (para tracking de orden)
    pub global_index: usize,
}

/// Catálogo completo de contenido procesado
#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessedFileManifest {
    version: u32,
    created_at: DateTime<Utc>,
    last_updated: DateTime<Utc>,

    /// Mapa de hash completo → entrada procesada (para búsqueda O(1) por contenido)
    pub entries_by_hash: BTreeMap<String, ProcessedFileEntry>,

    /// Índice de nombre final → hash (para búsqueda por nombre)
    pub entries_by_name: BTreeMap<String, String>,
}

impl ProcessedFileManifest {
    pub fn new() -> Self {
        Self {
            version: MANIFEST_VERSION,
            created_at: Utc::now(),
            last_updated: Utc::now(),
            entries_by_hash: BTreeMap::new(),
            entries_by_name: BTreeMap::new(),
        }
    }
}

impl Default for ProcessedFileManifest {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessedFileManifest {
    pub fn load_or_create_at(manifest_path: &Path) -> Result<Self> {

        if manifest_path.exists() {
            let content = fs::read_to_string(manifest_path)?;
            let manifest: ProcessedFileManifest = match serde_json::from_str(&content) {
                Ok(parsed) => parsed,
                Err(e) => {
                    log::warn!(
                        "Manifest corrupt or unreadable at {}: {}. Recreating empty manifest.",
                        manifest_path.display(),
                        e
                    );
                    let corrupt_path = manifest_path.with_extension("corrupt");
                    let _ = fs::rename(manifest_path, &corrupt_path);
                    return Ok(Self::new());
                }
            };

            if manifest.version != MANIFEST_VERSION {
                log::warn!(
                    "Manifest version mismatch: expected {}, got {}. Will continue with existing manifest.",
                    MANIFEST_VERSION,
                    manifest.version
                );
            }

            Ok(manifest)
        } else {
            Ok(Self::new())
        }
    }

    pub fn load_or_create(usb_mount: &Path) -> Result<Self> {
        let manifest_path = usb_mount.join(MANIFEST_FILENAME);
        Self::load_or_create_at(&manifest_path)
    }

    /// Registrar una canción ya procesada
    pub fn register_processed_file(
        &mut self,
        final_name: String,
        content_hash: String,
        size_bytes: u64,
        usb_relative_path: String,
        global_index: usize,
    ) {
        let entry = ProcessedFileEntry {
            final_name: final_name.clone(),
            content_hash: content_hash.clone(),
            size_bytes,
            usb_relative_path,
            processed_at: Utc::now(),
            global_index,
        };

        self.entries_by_hash.insert(content_hash.clone(), entry);
        self.entries_by_name.insert(final_name, content_hash);
        self.last_updated = Utc::now();
    }

    /// Consultar si un contenido ya fue procesado (por hash)
    pub fn is_content_already_processed(&self, content_hash: &str) -> bool {
        self.entries_by_hash.contains_key(content_hash)
    }

    /// Obtener entrada procesada por hash
    pub fn get_by_hash(&self, content_hash: &str) -> Option<&ProcessedFileEntry> {
        self.entries_by_hash.get(content_hash)
    }

    /// Cantidad total de canciones en el catálogo
    pub fn total_processed(&self) -> usize {
        self.entries_by_hash.len()
    }

    /// Guardar manifest a la USB de forma atómica
    pub fn save_to_path(&self, manifest_path: &Path) -> Result<()> {
        let tmp_path = manifest_path.with_extension("tmp");

        let json_content = serde_json::to_string_pretty(self)?;
        let mut tmp_file = File::create(&tmp_path)?;
        tmp_file.write_all(json_content.as_bytes())?;
        tmp_file.sync_all()?;
        drop(tmp_file);

        fs::rename(&tmp_path, manifest_path)?;

        if let Some(parent) = manifest_path.parent() {
            if let Ok(dir) = File::open(parent) {
                let _ = dir.sync_all();
            }
        }

        Ok(())
    }

    pub fn save_to_usb(&self, usb_mount: &Path) -> Result<()> {
        let manifest_path = usb_mount.join(MANIFEST_FILENAME);
        self.save_to_path(&manifest_path)
    }

    /// Reconstruye un baseline de manifest escaneando la USB (VOL_XX/*).
    pub fn rebuild_from_usb(usb_mount: &Path) -> Result<Self> {
        let mut rebuilt = Self::new();

        for entry in fs::read_dir(usb_mount)? {
            let entry = entry?;
            let volume_path = entry.path();
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let Some(volume_name) = volume_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !volume_name.starts_with("VOL_") {
                continue;
            }

            for file_entry in fs::read_dir(&volume_path)? {
                let file_entry = file_entry?;
                if !file_entry.file_type()?.is_file() {
                    continue;
                }

                let file_path = file_entry.path();
                let file_name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("audio.mp3")
                    .to_string();

                let content_hash = compute_file_sha256(&file_path)?;
                let size_bytes = file_entry.metadata()?.len();
                let usb_relative_path = file_path
                    .strip_prefix(usb_mount)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file_name.clone());
                let global_index = parse_global_prefix_index(&file_name).unwrap_or(0);

                rebuilt.register_processed_file(
                    file_name,
                    content_hash,
                    size_bytes,
                    usb_relative_path,
                    global_index,
                );
            }
        }

        Ok(rebuilt)
    }
}

fn parse_global_prefix_index(filename: &str) -> Option<usize> {
    let (prefix, _) = filename.split_once('_')?;
    prefix.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_manifest_register_and_query() {
        let mut manifest = ProcessedFileManifest::new();

        manifest.register_processed_file(
            "001_song_abcd1234.mp3".to_string(),
            "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234".to_string(),
            5_000_000,
            "VOL_01/001_song_abcd1234.mp3".to_string(),
            0,
        );

        assert!(manifest.is_content_already_processed(
            "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234"
        ));
        assert!(!manifest.is_content_already_processed("nonexistent"));
        assert_eq!(manifest.total_processed(), 1);
    }

    #[test]
    fn test_manifest_save_and_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut manifest = ProcessedFileManifest::new();
        let manifest_path = temp_dir.path().join("manifest_test.json");

        manifest.register_processed_file(
            "001_song_abcd1234.mp3".to_string(),
            "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234".to_string(),
            5_000_000,
            "VOL_01/001_song_abcd1234.mp3".to_string(),
            0,
        );

        manifest.save_to_path(&manifest_path)?;

        let loaded = ProcessedFileManifest::load_or_create_at(&manifest_path)?;
        assert_eq!(loaded.total_processed(), 1);
        assert!(loaded.is_content_already_processed(
            "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234"
        ));

        Ok(())
    }

    #[test]
    fn test_manifest_load_or_create_missing_file_returns_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let loaded =
            ProcessedFileManifest::load_or_create_at(&temp_dir.path().join("missing.json"))?;

        assert_eq!(loaded.total_processed(), 0);
        assert!(loaded.entries_by_hash.is_empty());
        assert!(loaded.entries_by_name.is_empty());

        Ok(())
    }

    #[test]
    fn test_manifest_register_same_hash_updates_without_duplication() {
        let mut manifest = ProcessedFileManifest::new();
        let hash = "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234";

        manifest.register_processed_file(
            "001_first_abcd1234.mp3".to_string(),
            hash.to_string(),
            1024,
            "VOL_01/001_first_abcd1234.mp3".to_string(),
            1,
        );
        manifest.register_processed_file(
            "002_second_abcd1234.mp3".to_string(),
            hash.to_string(),
            2048,
            "VOL_01/002_second_abcd1234.mp3".to_string(),
            2,
        );

        assert_eq!(manifest.total_processed(), 1);
        assert_eq!(manifest.entries_by_name.len(), 2);
        assert_eq!(manifest.get_by_hash(hash).map(|e| e.size_bytes), Some(2048));
    }

    #[test]
    fn test_manifest_load_or_create_recovers_from_corrupt_json() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest_path = temp_dir.path().join("manifest_corrupt.json");
        fs::write(&manifest_path, "{invalid-json")?;

        let loaded = ProcessedFileManifest::load_or_create_at(&manifest_path)?;

        assert_eq!(loaded.total_processed(), 0);
        assert!(temp_dir.path().join("manifest_corrupt.corrupt").exists());
        assert!(!manifest_path.exists());

        Ok(())
    }

    #[test]
    fn test_manifest_register_updates_last_updated() {
        let mut manifest = ProcessedFileManifest::new();
        let before = manifest.last_updated;

        manifest.register_processed_file(
            "001_song_abcd1234.mp3".to_string(),
            "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234".to_string(),
            5_000_000,
            "VOL_01/001_song_abcd1234.mp3".to_string(),
            0,
        );

        assert!(manifest.last_updated >= before);
        assert!(manifest.last_updated - before < Duration::seconds(5));
    }

    #[test]
    fn test_manifest_rebuild_from_usb_scans_volumes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let vol_01 = temp_dir.path().join("VOL_01");
        let vol_02 = temp_dir.path().join("VOL_02");
        let ignored = temp_dir.path().join("misc");
        fs::create_dir_all(&vol_01)?;
        fs::create_dir_all(&vol_02)?;
        fs::create_dir_all(&ignored)?;

        fs::write(vol_01.join("001_track_abcd1234.mp3"), b"one")?;
        fs::write(vol_02.join("099_song_ffff0000.mp3"), b"two")?;
        fs::write(ignored.join("not_in_manifest.mp3"), b"three")?;

        let rebuilt = ProcessedFileManifest::rebuild_from_usb(temp_dir.path())?;

        assert_eq!(rebuilt.total_processed(), 2);
        assert!(rebuilt.entries_by_name.contains_key("001_track_abcd1234.mp3"));
        assert!(rebuilt.entries_by_name.contains_key("099_song_ffff0000.mp3"));

        Ok(())
    }
}
