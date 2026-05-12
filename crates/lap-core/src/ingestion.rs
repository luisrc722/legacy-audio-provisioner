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
use crate::sanitizer;
use crate::security::{validate_filename_comprehensive, validate_path_containment};

const LEGACY_MAX_FILENAME_BYTES: usize = 32;

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
    ingest_audio_files_with_progress(from, to, json_mode, |_, _, _| {})
}

pub fn ingest_audio_files_with_progress<F>(
    from: &Path,
    to: &Path,
    json_mode: bool,
    mut on_progress: F,
) -> Result<IngestManifest>
where
    F: FnMut(usize, usize, &str),
{
    let total = audio_discovery::count_audio_files(from)?;

    if total == 0 {
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

    info!(
        "Iniciando ingesta: {} archivos desde '{}' hacia '{}'",
        total,
        from.display(),
        to.display()
    );

    let mut ingested: Vec<IngestedFile> = Vec::with_capacity(total);
    let mut total_bytes = 0u64;
    let mut used_names: HashSet<String> = HashSet::new();
    let mut files_processed = 0usize;

    audio_discovery::visit_audio_files(from, |audio_file| {
        let original_name = audio_file.filename.clone();

        // 1) Sanitizacion + truncamiento legacy (32 chars max, preservando extension)
        // 2) Mutacion de estado: a partir de aqui solo se usa el nombre saneado
        let mut base_name = sanitize_for_legacy_transfer(&original_name);
        if base_name.is_empty() {
            base_name = "audio.mp3".to_string();
        }

        // 3) Validacion sobre el valor mutado/saneado (nunca sobre el original)
        validate_filename_comprehensive(&base_name).with_context(|| {
            format!(
                "Nombre de archivo contiene caracteres peligrosos (R-35): original='{}' saneado='{}'",
                original_name, base_name
            )
        })?;

        let staged_name = resolve_collision_free_name(&base_name, &mut used_names);

        // R-34: Validar que la ruta de destino no escapa del directorio de staging
        let staged_path =
            validate_path_containment(to, Path::new(&staged_name)).with_context(|| {
                format!(
                    "Path traversal attack attempt detected (R-34): {} -> {}",
                    audio_file.path.display(),
                    staged_name
                )
            })?;

        fs::copy(&audio_file.path, &staged_path).with_context(|| {
            format!(
                "Error copiando '{}' -> '{}'",
                audio_file.path.display(),
                staged_path.display()
            )
        })?;

        let sha256_hex = sha256_of_file(&staged_path)?;
        total_bytes += audio_file.size_bytes;
        files_processed += 1;

        on_progress(files_processed, total, &staged_name);

        IpcEvent::Progress {
            files_processed,
            total_files: total,
            percentage: (files_processed as f64 / total as f64) * 100.0,
            current_file: staged_name.clone(),
            eta_seconds: 0,
        }
        .emit(json_mode);

        ingested.push(IngestedFile {
            source_path: audio_file.path.clone(),
            staged_path,
            sha256_hex,
        });

        Ok(())
    })?;

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

fn sanitize_for_legacy_transfer(raw_name: &str) -> String {
    let raw_path = Path::new(raw_name);
    let raw_stem = raw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw_name);
    let raw_ext = raw_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let mut safe_stem = sanitizer::sanitize_filename(raw_stem);
    if safe_stem.is_empty() {
        safe_stem = "audio".to_string();
    }

    let mut safe_ext = sanitizer::sanitize_filename(raw_ext);
    safe_ext.retain(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    let ext_part = if safe_ext.is_empty() {
        String::new()
    } else {
        format!(".{}", safe_ext)
    };

    // Mantiene la extension cuando sea posible y respeta el limite legacy.
    if ext_part.len() >= LEGACY_MAX_FILENAME_BYTES {
        return safe_stem.chars().take(LEGACY_MAX_FILENAME_BYTES).collect();
    }

    let available_stem_len = LEGACY_MAX_FILENAME_BYTES - ext_part.len();
    safe_stem = safe_stem.chars().take(available_stem_len).collect();

    if safe_stem.is_empty() {
        safe_stem = "a".to_string();
    }

    format!("{}{}", safe_stem, ext_part)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_sanitize_for_legacy_transfer_truncates_to_32_and_keeps_extension() {
        let input = "Cancion super_larga con simbolos !!! y espacios.mp3";
        let result = sanitize_for_legacy_transfer(input);

        assert!(result.len() <= 32);
        assert!(result.ends_with(".mp3"));
        assert!(result.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'));
    }

    #[test]
    fn test_sanitize_for_legacy_transfer_uses_fallback_stem_when_empty() {
        let input = "¡¡¡###@@@.mp3";
        let result = sanitize_for_legacy_transfer(input);

        assert_eq!(result, "audio.mp3");
    }

    #[test]
    fn test_ingest_audio_files_with_progress_processes_one_by_one() -> Result<()> {
        let source = TempDir::new()?;
        let staging = TempDir::new()?;

        fs::write(source.path().join("01 - canción.mp3"), b"abc")?;
        fs::write(source.path().join("02 - otra.mp3"), b"def")?;

        let mut progress_events = Vec::new();
        let manifest = ingest_audio_files_with_progress(
            source.path(),
            staging.path(),
            false,
            |processed, total, current_file| {
                progress_events.push((processed, total, current_file.to_string()));
            },
        )?;

        assert_eq!(manifest.files.len(), 2);
        assert_eq!(progress_events.len(), 2);
        assert_eq!(progress_events[0].0, 1);
        assert_eq!(progress_events[0].1, 2);
        assert!(progress_events[0].2.ends_with(".mp3"));
        assert_eq!(progress_events[1].0, 2);

        Ok(())
    }
}
