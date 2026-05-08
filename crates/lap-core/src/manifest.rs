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
    pub fn load_or_create(usb_mount: &Path) -> Result<Self> {
        let manifest_path = usb_mount.join(MANIFEST_FILENAME);

        if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            let manifest: ProcessedFileManifest = match serde_json::from_str(&content) {
                Ok(parsed) => parsed,
                Err(e) => {
                    log::warn!(
                        "Manifest corrupt or unreadable at {}: {}. Recreating empty manifest.",
                        manifest_path.display(),
                        e
                    );
                    let corrupt_path = manifest_path.with_extension("corrupt");
                    let _ = fs::rename(&manifest_path, &corrupt_path);
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
    pub fn save_to_usb(&self, usb_mount: &Path) -> Result<()> {
        let manifest_path = usb_mount.join(MANIFEST_FILENAME);
        let tmp_path = manifest_path.with_extension("tmp");

        let json_content = serde_json::to_string_pretty(self)?;
        let mut tmp_file = File::create(&tmp_path)?;
        tmp_file.write_all(json_content.as_bytes())?;
        tmp_file.sync_all()?;
        drop(tmp_file);

        fs::rename(&tmp_path, &manifest_path)?;

        if let Some(parent) = manifest_path.parent() {
            if let Ok(dir) = File::open(parent) {
                let _ = dir.sync_all();
            }
        }

        Ok(())
    }
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

        manifest.register_processed_file(
            "001_song_abcd1234.mp3".to_string(),
            "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234".to_string(),
            5_000_000,
            "VOL_01/001_song_abcd1234.mp3".to_string(),
            0,
        );

        manifest.save_to_usb(temp_dir.path())?;

        let loaded = ProcessedFileManifest::load_or_create(temp_dir.path())?;
        assert_eq!(loaded.total_processed(), 1);
        assert!(loaded.is_content_already_processed(
            "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234"
        ));

        Ok(())
    }

    #[test]
    fn test_manifest_load_or_create_missing_file_returns_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let loaded = ProcessedFileManifest::load_or_create(temp_dir.path())?;

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
        let manifest_path = temp_dir.path().join(MANIFEST_FILENAME);
        fs::write(&manifest_path, "{invalid-json")?;

        let loaded = ProcessedFileManifest::load_or_create(temp_dir.path())?;

        assert_eq!(loaded.total_processed(), 0);
        assert!(temp_dir.path().join(".provisioning_manifest.corrupt").exists());
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
}
