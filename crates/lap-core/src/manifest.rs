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

    /// Cargar manifest desde la USB (o crear uno vacío si no existe)
    pub fn load_or_create(usb_mount: &Path) -> Result<Self> {
        let manifest_path = usb_mount.join(MANIFEST_FILENAME);

        if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            let manifest: ProcessedFileManifest = serde_json::from_str(&content)?;

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
}
