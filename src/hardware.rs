//! R-04: Deteccion Dinamica de Hardware
//!
//! Implementa validacion estricta de particiones montadas interactuando
//! directamente con los descriptores del kernel de Linux (/proc/mounts y /sys/block).
//!
//! Garantiza que no se muten discos de sistema (ext4/btrfs/nvme no extraibles).

use anyhow::{anyhow, Context, Result};
use crate::error::ProvisioningError;
use log::{debug, info, warn};
use nix::sys::statvfs::statvfs;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device_path: PathBuf,
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub is_removable: bool,
}

impl DeviceInfo {
    pub fn is_valid_for_provisioning(&self) -> Result<()> {
        if self.fs_type != "vfat" && self.fs_type != "fat32" {
            return Err(anyhow!(
                "Hardware Safety Lock: Filesystem '{}' detectado en '{}'. Solo se permite FAT32 (vfat) para evitar corrupcion del disco de sistema.",
                self.fs_type,
                self.mount_point.display()
            ));
        }

        if !self.is_removable {
            return Err(anyhow!(
                "Hardware Safety Lock: El dispositivo '{}' no esta marcado como removible en el kernel. Operacion abortada.",
                self.device_path.display()
            ));
        }

        Ok(())
    }

    pub fn size_gb(&self) -> f64 {
        self.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn requires_confirmation(&self) -> bool {
        self.size_gb() > 64.0
    }
}

/// Extrae el nombre del bloque padre (ej. sdb1 -> sdb, mmcblk0p1 -> mmcblk0).
fn get_parent_block_device(partition_name: &str) -> String {
    if partition_name.starts_with("nvme") || partition_name.starts_with("mmcblk") {
        // Formato: mmcblk0p1 -> mmcblk0 | nvme0n1p1 -> nvme0n1
        if let Some(pos) = partition_name.rfind('p') {
            return partition_name[..pos].to_string();
        }
    }

    // Formato SATA/USB: sdb1 -> sdb
    partition_name
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}

/// Verifica si el bloque padre esta marcado como removible por el kernel.
fn is_device_removable(device_name: &str) -> bool {
    let parent = get_parent_block_device(device_name);
    let sysfs_path = format!("/sys/block/{}/removable", parent);

    match fs::read_to_string(&sysfs_path) {
        Ok(content) => content.trim() == "1",
        Err(_) => {
            debug!("No se pudo leer el flag removible en {}", sysfs_path);
            false
        }
    }
}

/// Analiza /proc/mounts y devuelve una lista de dispositivos de bloque montados.
fn get_mounted_devices() -> Result<Vec<DeviceInfo>> {
    let mounts_content = fs::read_to_string("/proc/mounts")
        .context("Fallo critico: No se pudo leer /proc/mounts")?;

    let mut devices = Vec::new();

    for line in mounts_content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let device_path = parts[0];
        let mount_point = parts[1];
        let fs_type = parts[2];

        // Ignorar pseudo-filesystems y loop devices.
        if !device_path.starts_with("/dev/") || device_path.starts_with("/dev/loop") {
            continue;
        }

        let stat = match statvfs(mount_point) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let block_size = stat.block_size() as u64;
        let total_bytes = stat.blocks() * block_size;
        let available_bytes = stat.blocks_available() * block_size;

        let device_name = device_path.rsplit('/').next().unwrap_or("");
        let is_removable = is_device_removable(device_name);

        devices.push(DeviceInfo {
            device_path: PathBuf::from(device_path),
            mount_point: PathBuf::from(mount_point),
            fs_type: fs_type.to_string(),
            total_bytes,
            available_bytes,
            is_removable,
        });
    }

    Ok(devices)
}

/// Detecta proactivamente dispositivos USB/SD validos (FAT32 + removible).
pub fn detect_usb_devices() -> Result<Vec<DeviceInfo>> {
    info!("Scanning /proc/mounts for FAT32 removable devices...");

    let mut valid_usbs = Vec::new();
    let all_devices = get_mounted_devices()?;

    for device in all_devices {
        if (device.fs_type == "vfat" || device.fs_type == "fat32") && device.is_removable {
            info!(
                "Detected valid target: {} at {} ({:.1} GB)",
                device.device_path.display(),
                device.mount_point.display(),
                device.size_gb()
            );
            valid_usbs.push(device);
        }
    }

    info!("Found {} valid removable FAT32 device(s)", valid_usbs.len());
    Ok(valid_usbs)
}

/// Validacion estricta para la CLI: comprueba el path inyectado por el usuario.
pub fn validate_device_path(target_mount_point: &Path) -> Result<DeviceInfo> {
    let target_str = target_mount_point
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();

    let all_devices = get_mounted_devices()?;

    let matched_device = all_devices
        .into_iter()
        .find(|d| d.mount_point.to_string_lossy().trim_end_matches('/') == target_str);

    match matched_device {
        Some(device) => {
            device.is_valid_for_provisioning()?;

            if device.requires_confirmation() {
                warn!(
                    "ALERTA: El dispositivo '{}' tiene un tamano de {:.1} GB. Asegurese de que es la USB correcta.",
                    device.mount_point.display(),
                    device.size_gb()
                );
            }

            Ok(device)
        }
        None => Err(anyhow!(
            "Ruta denegada: '{}' no corresponde a ningun dispositivo de bloque montado en /proc/mounts. No intente ejecutar el aprovisionador sobre directorios locales.",
            target_str
        )),
    }
}

const LOCK_FILE_NAME: &str = ".lap_provisioning.lock";

/// Estructura RAII para exclusión mutua de procesos sobre la misma USB.
/// Implementa el patrón RAII (Resource Acquisition Is Initialization) mediante
/// el trait Drop para garantizar que el archivo de bloqueo se limpia automáticamente.
pub struct ProvisioningLock {
    lock_path: PathBuf,
}

impl ProvisioningLock {
    /// Intenta adquirir un bloqueo exclusivo en la raíz del dispositivo.
    /// Falla (Fail-Fast) si otro proceso activo ya lo tiene.
    ///
    /// # Arguments
    /// * `usb_mount` - Ruta de montaje del dispositivo USB
    ///
    /// # Returns
    /// * `Ok(Self)` - Bloqueo adquirido exitosamente
    /// * `Err(_)` - Si ya existe un bloqueo de un proceso activo
    pub fn acquire(usb_mount: &Path) -> std::result::Result<Self, ProvisioningError> {
        let lock_path = usb_mount.join(LOCK_FILE_NAME);

        // 1. Verificar si ya existe un bloqueo previo
        if lock_path.exists() {
            let pid_str = fs::read_to_string(&lock_path).unwrap_or_default();

            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                // Comprobación nativa en Linux: Si /proc/PID existe, el proceso está vivo.
                let proc_dir = PathBuf::from(format!("/proc/{}", pid));

                if proc_dir.exists() {
                    return Err(ProvisioningError::ConcurrencyError {
                        details: format!(
                            "Otra instancia (PID {}) ya esta provisionando esta USB. Operacion abortada.",
                            pid
                        ),
                    });
                } else {
                    // Lock huérfano (El proceso murió por SIGKILL o crash).
                    warn!(
                        "Lock huérfano detectado de PID muerto ({}). Purgando...",
                        pid
                    );
                    let _ = fs::remove_file(&lock_path);
                }
            } else {
                // Archivo lock ilegible o corrupto. Se destruye.
                let _ = fs::remove_file(&lock_path);
            }
        }

        // 2. Escribir nuestro propio bloqueo
        let my_pid = process::id();
        fs::write(&lock_path, my_pid.to_string()).map_err(|e| ProvisioningError::ProvisioningFailed {
            details: format!(
                "Fallo la escritura del archivo de bloqueo de concurrencia en {}: {}",
                lock_path.display(),
                e
            ),
        })?;

        info!("Hardware Lock adquirido con éxito (PID: {})", my_pid);

        Ok(Self { lock_path })
    }
}

/// R-20: Test de escritura atomica para detectar sistemas de archivos Read-Only (EROFS).
/// Falla proactivamente si la memoria tiene el 'dirty bit' activo por extraccion insegura.
pub fn assert_rw_filesystem(usb_mount: &Path) -> std::result::Result<(), ProvisioningError> {
    let test_file = usb_mount.join(".fat32_dirty_test");

    // Intentamos escribir un solo byte.
    match fs::write(&test_file, b"1") {
        Ok(_) => {
            // I/O exitosa. La memoria es de lectura/escritura. Limpiamos el rastro.
            let _ = fs::remove_file(&test_file);
            Ok(())
        }
        Err(e) => {
            // EROFS en Linux suele reportar OS Error 30, o genericamente PermissionDenied.
            let is_ro =
                e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(30);

            if is_ro {
                Err(ProvisioningError::ReadOnlyFilesystem {
                    details: format!(
                        "El sistema de archivos en {} esta en modo Solo Lectura (Dirty Bit). Kernel: {}",
                        usb_mount.display(),
                        e
                    ),
                })
            } else {
                // Otro tipo de fallo de hardware (desconexion fisica en curso, etc.)
                Err(ProvisioningError::ProvisioningFailed {
                    details: format!("Fallo el test de escritura en la USB: {}", e),
                })
            }
        }
    }
}

/// Implementación estricta de RAII.
/// Cuando la instancia `ProvisioningLock` sale de scope, Rust invoca esto automáticamente.
impl Drop for ProvisioningLock {
    fn drop(&mut self) {
        if self.lock_path.exists() {
            if let Err(e) = fs::remove_file(&self.lock_path) {
                log::error!(
                    "Fallo crítico al liberar el lock {}: {}",
                    self.lock_path.display(),
                    e
                );
            } else {
                log::debug!("Hardware Lock liberado correctamente.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parent_block_device_parsing() {
        assert_eq!(get_parent_block_device("sdb1"), "sdb");
        assert_eq!(get_parent_block_device("mmcblk0p1"), "mmcblk0");
        assert_eq!(get_parent_block_device("nvme0n1p1"), "nvme0n1");
    }

    #[test]
    fn test_device_info_creation() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            available_bytes: 8 * 1024 * 1024 * 1024,
            is_removable: true,
        };

        assert!(device.is_valid_for_provisioning().is_ok());
        assert!(!device.requires_confirmation());
        assert!(device.size_gb() < 20.0);
    }

    #[test]
    fn test_large_device_requires_confirmation() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 128 * 1024 * 1024 * 1024,
            available_bytes: 64 * 1024 * 1024 * 1024,
            is_removable: true,
        };

        assert!(device.requires_confirmation());
    }

    #[test]
    fn test_non_removable_device_fails() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sda1"),
            mount_point: PathBuf::from("/mnt/internal"),
            fs_type: "vfat".to_string(),
            total_bytes: 1024 * 1024 * 1024,
            available_bytes: 512 * 1024 * 1024,
            is_removable: false,
        };

        assert!(device.is_valid_for_provisioning().is_err());
    }

    #[test]
    fn test_wrong_filesystem_fails() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "ntfs".to_string(),
            total_bytes: 1024 * 1024 * 1024,
            available_bytes: 512 * 1024 * 1024,
            is_removable: true,
        };

        assert!(device.is_valid_for_provisioning().is_err());
    }

    #[test]
    fn test_detect_usb_devices_real() {
        match detect_usb_devices() {
            Ok(devices) => {
                println!("Found {} USB devices", devices.len());
                for device in devices {
                    println!(
                        "  Device: {} -> {} ({})",
                        device.device_path.display(),
                        device.mount_point.display(),
                        device.size_gb()
                    );
                    assert!(device.is_valid_for_provisioning().is_ok());
                    assert!(device.fs_type == "vfat" || device.fs_type == "fat32");
                    assert!(device.is_removable);
                }
            }
            Err(e) => {
                eprintln!("Could not detect USB devices: {}", e);
            }
        }
    }
}
