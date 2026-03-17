//! R-31: Pipeline de Ingesta Local (Staging)
//!
//! Copia archivos de audio desde el dispositivo fuente (USB sucia) a un
//! directorio de trabajo temporal en almacenamiento local del host para su posterior normalizacion
//! y re-inyeccion via `provision`.
//!
//! Garantias:
//! - El dispositivo de origen NO se modifica en ningun momento.
//! - Manifest SHA256 con trazabilidad completa fuente -> staging.
//! - Resolucion de colisiones de nombre (mismo filename en subdirectorios distintos).

use anyhow::{Context, Result};
use log::info;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::audio_discovery;
use crate::ipc::IpcEvent;

/// Entrada del manifiesto de ingesta: trazabilidad origen -> staging + SHA256.
#[derive(Debug, Clone)]
pub struct IngestedFile {
    /// Ruta original en el dispositivo fuente.
    pub source_path: PathBuf,
    /// Ruta del archivo en el directorio de staging local del host.
    pub staged_path: PathBuf,
    /// SHA256 hex del archivo copiado (para verificacion posterior).
    pub sha256_hex: String,
}

/// Resultado completo de la operacion de ingesta.
#[derive(Debug)]
pub struct IngestManifest {
    /// Directorio de staging en host storage donde se copiaron los archivos.
    pub staging_dir: PathBuf,
    /// Archivos copiados con su trazabilidad.
    pub files: Vec<IngestedFile>,
    /// Bytes totales copiados.
    pub total_bytes: u64,
}

/// Copia todos los archivos de audio encontrados en `from` hacia `to`.
///
/// - Solo lectura del origen (no muta la USB sucia).
/// - Resuelve colisiones de nombre con sufijo `_001`, `_002`, etc.
/// - Retorna un manifiesto con trazabilidad fuente -> staging + SHA256.
pub fn ingest_audio_files(from: &Path, to: &Path, json_mode: bool) -> Result<IngestManifest> {
    let report = audio_discovery::discover_audio_files(from)?;

    if report.audio_files.is_empty() {
        return Err(anyhow::anyhow!(
            "No se encontraron archivos de audio en '{}'",
            from.display()
        ));
    }

    fs::create_dir_all(to).with_context(|| {
        format!(
            "No se pudo crear el directorio de staging '{}'",
            to.display()
        )
    })?;

    let total = report.audio_files.len();
    info!(
        "Iniciando ingesta: {} archivos desde '{}' hacia '{}'",
        total,
        from.display(),
        to.display()
    );

    let mut ingested: Vec<IngestedFile> = Vec::with_capacity(total);
    let mut total_bytes = 0u64;
    let mut used_names: HashSet<String> = HashSet::new();

    for (idx, audio_file) in report.audio_files.iter().enumerate() {
        let base_name = audio_file
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let staged_name = resolve_collision_free_name(&base_name, &mut used_names);
        let staged_path = to.join(&staged_name);

        fs::copy(&audio_file.path, &staged_path).with_context(|| {
            format!(
                "Error copiando '{}' -> '{}'",
                audio_file.path.display(),
                staged_path.display()
            )
        })?;

        let sha256_hex = sha256_of_file(&staged_path)?;
        total_bytes += audio_file.size_bytes;

        IpcEvent::Progress {
            files_processed: idx + 1,
            total_files: total,
            percentage: ((idx + 1) as f64 / total as f64) * 100.0,
            current_file: staged_name,
            eta_seconds: 0,
        }
        .emit(json_mode);

        ingested.push(IngestedFile {
            source_path: audio_file.path.clone(),
            staged_path,
            sha256_hex,
        });
    }

    info!(
        "Ingesta completada: {} archivos, {:.2} MB",
        ingested.len(),
        total_bytes as f64 / 1_048_576.0
    );

    Ok(IngestManifest {
        staging_dir: to.to_path_buf(),
        files: ingested,
        total_bytes,
    })
}

/// Resuelve colisiones de nombre en el staging flat: si `base_name` ya fue
/// usado, agrega sufijo `_001`, `_002`, etc. antes de la extension.
fn resolve_collision_free_name(base_name: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base_name.to_string()) {
        return base_name.to_string();
    }

    let p = Path::new(base_name);
    let stem = p
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1u32;
    loop {
        let candidate = format!("{}_{:03}{}", stem, counter, ext);
        if used.insert(candidate.clone()) {
            return candidate;
        }
        counter += 1;
    }
}

fn sha256_of_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("No se pudo abrir '{}' para hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
