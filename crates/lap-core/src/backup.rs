//! R-05: Preservación y Backup (Data Integrity)
//!
//! Requisitos:
//! - Crear directorio de backup estable por dispositivo: ~/usb_backup_<device_identity>/
//! - Copiar archivos físicamente con verificación de checksums SHA256
//! - Fallar proactivamente si espacio en disco < tamaño requerido (statvfs)
//! - Manejar colisiones de nombres preservando integridad

use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BackupMetadata {
    /// Directorio de backup
    pub backup_dir: PathBuf,

    /// Cantidad de archivos respaldados
    pub file_count: usize,

    /// Tamaño total respaldado (bytes)
    pub total_size: u64,

    /// Mapa de archivos y checksums SHA256 (para verificación post-copia)
    pub checksums: std::collections::HashMap<PathBuf, String>,
}

impl BackupMetadata {
    /// Crear o reutilizar un directorio de backup estable genérico dentro de base_dir.
    pub fn new(base_dir: &Path) -> Result<Self> {
        Self::new_for_target(base_dir, "generic_device")
    }

    /// Crear o reutilizar un directorio de backup estable para un dispositivo objetivo.
    pub fn new_for_target(base_dir: &Path, target_key: &str) -> Result<Self> {
        let slug = sanitize_target_key(target_key);
        let backup_dir = base_dir.join(format!("usb_backup_{}", slug));

        fs::create_dir_all(&backup_dir).with_context(|| {
            format!(
                "Failed to create stable backup directory: {}",
                backup_dir.display()
            )
        })?;

        info!(
            "Stable backup directory initialized for target '{}': {}",
            target_key,
            backup_dir.display()
        );

        Ok(BackupMetadata {
            backup_dir,
            file_count: 0,
            total_size: 0,
            checksums: std::collections::HashMap::new(),
        })
    }

    /// [R-06-004] Backup with Checksums
    /// Legacy cross-ref: R-05.
    /// Precondición: el directorio base de respaldo del host es escribible o puede crearse de manera segura.
    /// Postcondición: existe un directorio estable reutilizable para el destino lógico actual.
    /// Invariante: la creación de respaldo nunca muta la carga USB/audio de origen; solo prepara persistencia local en el host.
    ///
    /// Crear o reutilizar un directorio de backup estable en base_dir especificado.
    pub fn new_with_base_dir(base_dir: Option<&Path>) -> Result<Self> {
        let base = if let Some(dir) = base_dir {
            dir.to_path_buf()
        } else {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map(PathBuf::from)
                .map_err(|_| anyhow!("Cannot determine user home directory"))?
        };

        Self::new_for_target(&base, "generic_device")
    }

    /// [R-06-004] Respaldo con Checksums
    /// Referencia legacy: R-05.
    /// Precondición: `source_path` existe y es legible del conjunto de origen confiable.
    /// Postcondición: existe una copia de respaldo local duradera en el host y su SHA256 está registrado en memoria.
    /// Invariante: los bytes de origen se copian textualmente; las colisiones se resuelven sin sobrescribir artefactos de respaldo previos.
    ///
    /// Copia un archivo al directorio de backup y calcula SHA256 en una sola pasada.
    /// Maneja colisiones de nombres añadiendo sufijo numérico.
    ///
    /// # Ejemplo de Colisión Resuelta
    /// - Archivo 1: /cd1/track.mp3 → usb_backup_<device_identity>/track.mp3
    /// - Archivo 2: /cd2/track.mp3 → usb_backup_<device_identity>/track_1.mp3
    pub fn backup_file(&mut self, source_path: &Path) -> Result<()> {
        let file_name = source_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid source path: {:?}", source_path))?
            .to_string_lossy()
            .to_string();

        let preferred_dest = self.backup_dir.join(&file_name);
        if preferred_dest.exists() && files_are_identical(source_path, &preferred_dest)? {
            info!(
                "Backup up-to-date (skipped): {}",
                source_path.display()
            );
            return Ok(());
        }

        let dest_path = if preferred_dest.exists() {
            unique_backup_destination(&self.backup_dir, source_path, &file_name)
        } else {
            preferred_dest
        };

        self.copy_file_to_destination(source_path, dest_path)
    }

    pub fn backup_file_preserving_tree(&mut self, source_path: &Path, root_path: &Path) -> Result<()> {
        let relative_path = source_path.strip_prefix(root_path).with_context(|| {
            format!(
                "Backup tree root '{}' does not contain source '{}'",
                root_path.display(),
                source_path.display()
            )
        })?;

        let file_name = relative_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid source path inside tree: {:?}", source_path))?
            .to_string_lossy()
            .to_string();

        let relative_parent = relative_path.parent().unwrap_or(Path::new(""));
        let target_dir = self.backup_dir.join(relative_parent);
        fs::create_dir_all(&target_dir).with_context(|| {
            format!(
                "Failed to create backup tree directory: {}",
                target_dir.display()
            )
        })?;

        // En backup con arbol preservado, la ruta relativa identifica univocamente al archivo.
        // Reutilizamos el mismo destino para evitar crecimiento sin limite entre ejecuciones.
        let dest_path = target_dir.join(&file_name);
        if dest_path.exists() && files_are_identical(source_path, &dest_path)? {
            info!(
                "Tree backup up-to-date (skipped): {}",
                source_path.display()
            );
            return Ok(());
        }

        self.copy_file_to_destination(source_path, dest_path)
    }

    fn copy_file_to_destination(&mut self, source_path: &Path, dest_path: PathBuf) -> Result<()> {

        // Lectura, escritura y hashing simultáneo para máxima eficiencia (una sola pasada)
        let mut source_file = File::open(source_path)
            .with_context(|| format!("Failed to open source file: {}", source_path.display()))?;

        let mut dest_file = File::create(&dest_path)
            .with_context(|| format!("Failed to create backup file: {}", dest_path.display()))?;

        let mut hasher = Sha256::new();
        let mut buffer = [0; 65536]; // Buffer de 64KB para I/O eficiente
        let mut file_size = 0u64;

        // Copiar y hashear en paralelo (streaming)
        loop {
            let bytes_read = source_file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            dest_file.write_all(&buffer[..bytes_read])?;
            hasher.update(&buffer[..bytes_read]);
            file_size += bytes_read as u64;
        }

        // Asegurar que la escritura llega al disco
        dest_file.sync_all()?;

        let checksum = hex::encode(hasher.finalize());
        self.checksums.insert(dest_path.clone(), checksum);
        self.file_count += 1;
        self.total_size += file_size;

        info!(
            "✓ Backed up: {} ({:.2} MB)",
            source_path.display(),
            file_size as f64 / (1024.0 * 1024.0)
        );

        Ok(())
    }

    /// [R-06-004] Backup with Checksums
    /// Legacy cross-ref: R-05.
    /// Precondición: `self.checksums` contiene el mapa de digests esperado para archivos previamente respaldados.
    /// Postcondición: retorna `true` solo si todos los artefactos de respaldo existen y coinciden con su SHA256 registrado.
    /// Invariante: ningún archivo de respaldo puede ser aceptado como válido sin acuerdo de checksum a nivel de byte.
    ///
    /// Verificar integridad de todo el backup comparando checksums
    pub fn verify_backup(&self) -> Result<bool> {
        info!("Verifying backup integrity of {} files...", self.file_count);

        if self.file_count == 0 {
            info!("Backup vacio: no hay archivos para verificar, se considera no-op valido.");
            return Ok(true);
        }

        for (file_path, expected_checksum) in &self.checksums {
            if !file_path.exists() {
                warn!("Backup file missing: {}", file_path.display());
                return Ok(false);
            }

            // Recalcular checksum del archivo respaldado
            let mut file = File::open(file_path)?;
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher)?;
            let actual_checksum = hex::encode(hasher.finalize());

            if actual_checksum != *expected_checksum {
                warn!(
                    "❌ Corruption detected in {}: expected {}, got {}",
                    file_path.display(),
                    expected_checksum,
                    actual_checksum
                );
                return Ok(false);
            }
        }

        info!(
            "✅ Backup verification passed! All {} checksums match.",
            self.file_count
        );
        Ok(true)
    }
}

fn unique_backup_destination(base_dir: &Path, source_path: &Path, file_name: &str) -> PathBuf {
    let mut dest_path = base_dir.join(file_name);
    let mut counter = 1;

    while dest_path.exists() {
        let stem = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("file")
            .to_string();
        let ext = source_path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();

        let new_name = if ext.is_empty() {
            format!("{}_{}", stem, counter)
        } else {
            format!("{}_{}.{}", stem, counter, ext)
        };

        dest_path = base_dir.join(new_name);
        counter += 1;
    }

    dest_path
}

fn files_are_identical(source_path: &Path, destination_path: &Path) -> Result<bool> {
    let source_meta = fs::metadata(source_path).with_context(|| {
        format!(
            "Failed to read source metadata for '{}'",
            source_path.display()
        )
    })?;
    let destination_meta = fs::metadata(destination_path).with_context(|| {
        format!(
            "Failed to read destination metadata for '{}'",
            destination_path.display()
        )
    })?;

    if source_meta.len() != destination_meta.len() {
        return Ok(false);
    }

    let source_hash = compute_sha256_for_path(source_path)?;
    let destination_hash = compute_sha256_for_path(destination_path)?;
    Ok(source_hash == destination_hash)
}

fn compute_sha256_for_path(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("Failed to open file for checksum: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn sanitize_target_key(target_key: &str) -> String {
    let mut slug = String::with_capacity(target_key.len());
    let mut last_was_separator = false;

    for ch in target_key.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            last_was_separator = false;
            ch.to_ascii_lowercase()
        } else if !last_was_separator {
            last_was_separator = true;
            '_'
        } else {
            continue;
        };

        slug.push(normalized);
    }

    let trimmed = slug.trim_matches('_');
    if trimmed.is_empty() {
        "device".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Verificar disponibilidad de espacio en disco antes de operaciones críticas
/// Usa statvfs en Unix para validar cuotas reales del kernel
#[cfg(unix)]
pub fn check_disk_space(required_bytes: u64, backup_path: &Path) -> Result<()> {
    use nix::sys::statvfs::statvfs;

    // Si el directorio aún no existe, verificar el padre
    let path_to_check = if backup_path.exists() {
        backup_path
    } else {
        backup_path.parent().unwrap_or(Path::new("/"))
    };

    let stat =
        statvfs(path_to_check).with_context(|| "Failed to execute statvfs for disk space check")?;

    // bavail: bloques disponibles para usuarios no-root (el límite real)
    let available_bytes = stat.block_size() as u64 * stat.blocks_available() as u64;

    if available_bytes < required_bytes {
        return Err(anyhow!(
            "❌ INSUFFICIENT DISK SPACE\n\
            Required: {:.2} GB\n\
            Available: {:.2} GB\n\
            Shortfall: {:.2} GB",
            required_bytes as f64 / 1_073_741_824.0,
            available_bytes as f64 / 1_073_741_824.0,
            (required_bytes.saturating_sub(available_bytes)) as f64 / 1_073_741_824.0
        ));
    }

    info!(
        "✅ Disk space check passed. Available: {:.2} GB (required: {:.2} GB)",
        available_bytes as f64 / 1_073_741_824.0,
        required_bytes as f64 / 1_073_741_824.0
    );

    Ok(())
}

/// Fallback para Windows (statvfs es Unix-only)
/// Se recomienda implementar con Windows API en el futuro
#[cfg(not(unix))]
pub fn check_disk_space(required_bytes: u64, _backup_path: &Path) -> Result<()> {
    warn!(
        "⚠️  Disk space validation is limited on non-Unix systems. \
        Proceeding with caution. Required: {:.2} GB",
        required_bytes as f64 / 1_073_741_824.0
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_backup_metadata_creation() -> Result<()> {
        let temp = TempDir::new()?;
        let backup = BackupMetadata::new_with_base_dir(Some(temp.path()))?;
        assert!(backup.backup_dir.exists());
        assert_eq!(backup.file_count, 0);
        assert_eq!(backup.total_size, 0);
        Ok(())
    }

    #[test]
    fn test_empty_backup_verification_is_valid_no_op() -> Result<()> {
        let temp = TempDir::new()?;
        let backup = BackupMetadata::new_with_base_dir(Some(temp.path()))?;

        assert!(backup.verify_backup()?);

        Ok(())
    }

    #[test]
    fn test_stable_backup_directory_reused_for_same_target() -> Result<()> {
        let temp = TempDir::new()?;

        let target_key = "CABINA_A__SanDisk__Ultra Fit__4C530001230101117391__ABCD-1234";
        let backup_a = BackupMetadata::new_for_target(temp.path(), target_key)?;
        let backup_b = BackupMetadata::new_for_target(temp.path(), target_key)?;

        assert_eq!(backup_a.backup_dir, backup_b.backup_dir);
        assert!(backup_a.backup_dir.exists());
        assert!(backup_a
            .backup_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .starts_with("usb_backup_cabina_a_sandisk_ultra_fit_4c530001230101117391_abcd_1234"));

        Ok(())
    }

    #[test]
    fn test_backup_file_copy() -> Result<()> {
        let temp_source = TempDir::new()?;
        let temp_backup = TempDir::new()?;
        let source_file = temp_source.path().join("test_audio.mp3");
        std::fs::write(&source_file, b"fake audio data 123")?;

        let mut backup = BackupMetadata::new(temp_backup.path())?;
        backup.backup_file(&source_file)?;

        assert_eq!(backup.file_count, 1);
        assert!(backup.total_size > 0);
        assert_eq!(backup.checksums.len(), 1);

        // Verificar que el archivo se copió
        let backed_file = backup.backup_dir.join("test_audio.mp3");
        assert!(backed_file.exists());
        let content = std::fs::read(&backed_file)?;
        assert_eq!(content, b"fake audio data 123");

        Ok(())
    }

    #[test]
    fn test_backup_file_preserves_tree_structure() -> Result<()> {
        let temp_source = TempDir::new()?;
        let temp_backup = TempDir::new()?;

        let nested_dir = temp_source.path().join("VOL_01").join("subdir");
        fs::create_dir_all(&nested_dir)?;
        let source_file = nested_dir.join("song.mp3");
        fs::write(&source_file, b"tree-data")?;

        let mut backup = BackupMetadata::new_for_target(temp_backup.path(), "device_tree")?;
        backup.backup_file_preserving_tree(&source_file, temp_source.path())?;

        let backed_file = backup
            .backup_dir
            .join("VOL_01")
            .join("subdir")
            .join("song.mp3");
        assert!(backed_file.exists());
        assert_eq!(fs::read(&backed_file)?, b"tree-data");
        Ok(())
    }

    #[test]
    fn test_backup_file_skips_if_same_content_already_exists() -> Result<()> {
        let temp_source = TempDir::new()?;
        let temp_backup = TempDir::new()?;

        let source_file = temp_source.path().join("stable_track.mp3");
        fs::write(&source_file, b"same-bytes")?;

        let mut backup = BackupMetadata::new(temp_backup.path())?;
        backup.backup_file(&source_file)?;
        backup.backup_file(&source_file)?;

        assert_eq!(backup.file_count, 1);
        let entries: Vec<_> = fs::read_dir(&backup.backup_dir)?.collect::<std::result::Result<_, _>>()?;
        assert_eq!(entries.len(), 1);

        Ok(())
    }

    #[test]
    fn test_backup_file_preserving_tree_rewrites_same_relative_path() -> Result<()> {
        let temp_source = TempDir::new()?;
        let temp_backup = TempDir::new()?;

        let nested_dir = temp_source.path().join("VOL_01").join("subdir");
        fs::create_dir_all(&nested_dir)?;
        let source_file = nested_dir.join("song.mp3");

        let mut backup = BackupMetadata::new_for_target(temp_backup.path(), "device_tree")?;

        fs::write(&source_file, b"old-data")?;
        backup.backup_file_preserving_tree(&source_file, temp_source.path())?;

        fs::write(&source_file, b"new-data")?;
        backup.backup_file_preserving_tree(&source_file, temp_source.path())?;

        let backed_file = backup
            .backup_dir
            .join("VOL_01")
            .join("subdir")
            .join("song.mp3");
        let colliding_file = backup
            .backup_dir
            .join("VOL_01")
            .join("subdir")
            .join("song_1.mp3");

        assert!(backed_file.exists());
        assert!(!colliding_file.exists());
        assert_eq!(fs::read(&backed_file)?, b"new-data");

        Ok(())
    }

    #[test]
    fn test_backup_collision_handling() -> Result<()> {
        let temp_source = TempDir::new()?;
        let temp_backup = TempDir::new()?;

        // Crear dos carpetas con archivos del mismo nombre
        let dir1 = temp_source.path().join("cd1");
        let dir2 = temp_source.path().join("cd2");
        std::fs::create_dir(&dir1)?;
        std::fs::create_dir(&dir2)?;

        let file1 = dir1.join("track01.mp3");
        let file2 = dir2.join("track01.mp3");
        std::fs::write(&file1, b"audio cd1")?;
        std::fs::write(&file2, b"audio cd2")?;

        let mut backup = BackupMetadata::new(temp_backup.path())?;
        backup.backup_file(&file1)?;
        backup.backup_file(&file2)?; // Debería crear track01_1.mp3

        assert_eq!(backup.file_count, 2);

        // Verificar que ambos archivos existen con nombres distintos
        let backed1 = backup.backup_dir.join("track01.mp3");
        let backed2 = backup.backup_dir.join("track01_1.mp3");

        assert!(backed1.exists());
        assert!(backed2.exists());

        let content1 = std::fs::read(&backed1)?;
        let content2 = std::fs::read(&backed2)?;
        assert_eq!(content1, b"audio cd1");
        assert_eq!(content2, b"audio cd2");

        Ok(())
    }

    #[test]
    fn test_backup_verify() -> Result<()> {
        let temp_source = TempDir::new()?;
        let temp_backup = TempDir::new()?;
        let source_file = temp_source.path().join("test.mp3");
        std::fs::write(&source_file, b"test audio")?;

        let mut backup = BackupMetadata::new(temp_backup.path())?;
        backup.backup_file(&source_file)?;

        // La verificación debe pasar
        assert!(backup.verify_backup()?);

        Ok(())
    }
}
