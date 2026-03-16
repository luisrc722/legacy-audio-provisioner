//! R-05: Preservación y Backup (Data Integrity)
//!
//! Requisitos:
//! - Crear directorio de backup con timestamp: ~/usb_backup_YYYYMMDD_HHMM/
//! - Copiar archivos físicamente con verificación de checksums SHA256
//! - Fallar proactivamente si espacio en disco < tamaño requerido (statvfs)
//! - Manejar colisiones de nombres preservando integridad

use anyhow::{anyhow, Context, Result};
use chrono::Local;
use log::{info, warn};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BackupMetadata {
    /// Directorio de backup
    pub backup_dir: PathBuf,

    /// Timestamp de creación (YYYYMMDD_HHMM)
    pub timestamp: String,

    /// Cantidad de archivos respaldados
    pub file_count: usize,

    /// Tamaño total respaldado (bytes)
    pub total_size: u64,

    /// Mapa de archivos y checksums SHA256 (para verificación post-copia)
    pub checksums: std::collections::HashMap<PathBuf, String>,
}

impl BackupMetadata {
    /// Crear nuevo backup con directorio timestamped dentro de base_dir
    pub fn new(base_dir: &Path) -> Result<Self> {
        Self::new_with_base_dir(Some(base_dir))
    }

    /// Crear nuevo backup con directorio timestamped en base_dir especificado
    pub fn new_with_base_dir(base_dir: Option<&Path>) -> Result<Self> {
        let now = Local::now();
        let timestamp = now.format("%Y%m%d_%H%M").to_string();

        let base = if let Some(dir) = base_dir {
            dir.to_path_buf()
        } else {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map(PathBuf::from)
                .map_err(|_| anyhow!("Cannot determine user home directory"))?
        };

        let backup_dir = base.join(format!("usb_backup_{}", timestamp));

        fs::create_dir_all(&backup_dir).with_context(|| {
            format!(
                "Failed to create backup directory: {}",
                backup_dir.display()
            )
        })?;

        info!("Backup directory initialized: {}", backup_dir.display());

        Ok(BackupMetadata {
            backup_dir,
            timestamp,
            file_count: 0,
            total_size: 0,
            checksums: std::collections::HashMap::new(),
        })
    }

    /// Copia un archivo al directorio de backup y calcula SHA256 en una sola pasada.
    /// Maneja colisiones de nombres añadiendo sufijo numérico.
    ///
    /// # Ejemplo de Colisión Resuelta
    /// - Archivo 1: /cd1/track.mp3 → usb_backup_YYYYMMDD_HHMM/track.mp3
    /// - Archivo 2: /cd2/track.mp3 → usb_backup_YYYYMMDD_HHMM/track_1.mp3
    pub fn backup_file(&mut self, source_path: &Path) -> Result<()> {
        let file_name = source_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid source path: {:?}", source_path))?
            .to_string_lossy()
            .to_string();

        // Resolver colisiones de nombres: track.mp3, track_1.mp3, track_2.mp3, ...
        let mut dest_path = self.backup_dir.join(&file_name);
        let mut counter = 1;

        while dest_path.exists() {
            let stem = source_path
                .file_stem()
                .unwrap()
                .to_string_lossy()
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

            dest_path = self.backup_dir.join(new_name);
            counter += 1;
        }

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

    /// Verificar integridad de todo el backup comparando checksums
    pub fn verify_backup(&self) -> Result<bool> {
        info!("Verifying backup integrity of {} files...", self.file_count);

        if self.file_count == 0 {
            warn!("⚠️  Backup is empty - nothing to verify");
            return Ok(false);
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
    fn test_timestamp_format() -> Result<()> {
        let temp = TempDir::new()?;
        let backup = BackupMetadata::new_with_base_dir(Some(temp.path()))?;
        // Timestamp should be YYYYMMDD_HHMM
        assert_eq!(backup.timestamp.len(), 13); // 8 (YYYYMMDD) + 1 (_) + 4 (HHMM)
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
