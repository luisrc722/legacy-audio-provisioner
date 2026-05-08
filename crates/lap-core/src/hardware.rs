//! R-04: Deteccion Dinamica de Hardware
//!
//! Implementa validacion estricta de particiones montadas interactuando
//! directamente con los descriptores del kernel de Linux (/proc/mounts y /sys/block).
//!
//! Garantiza que no se muten discos de sistema (ext4/btrfs/nvme no extraibles).

use crate::error::ProvisioningError;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use nix::sys::statvfs::statvfs;
use std::fs::File;
use std::io::Read;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

pub const LEGACY_FAT32_ALLOCATION_UNIT_BYTES: u64 = 32 * 1024;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device_path: PathBuf,
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub is_removable: bool,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub fs_uuid: Option<String>,
    pub fs_label: Option<String>,
    pub allocation_unit_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyFormatReport {
    pub allocation_unit_bytes: Option<u64>,
    pub is_legacy_cache_optimized: bool,
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

    pub fn backup_identity_key(&self) -> String {
        let mut parts = Vec::new();

        push_identity_part(&mut parts, self.fs_label.as_deref());
        push_identity_part(&mut parts, self.vendor.as_deref());
        push_identity_part(&mut parts, self.model.as_deref());
        push_identity_part(&mut parts, self.serial.as_deref());
        push_identity_part(&mut parts, self.fs_uuid.as_deref());

        if parts.is_empty() {
            parts.push(self.device_path.display().to_string());
            parts.push(self.total_bytes.to_string());
        }

        parts.join("__")
    }

    pub fn legacy_format_report(&self) -> LegacyFormatReport {
        let is_legacy_cache_optimized = self
            .allocation_unit_bytes
            .map(|value| value == LEGACY_FAT32_ALLOCATION_UNIT_BYTES)
            .unwrap_or(false);

        LegacyFormatReport {
            allocation_unit_bytes: self.allocation_unit_bytes,
            is_legacy_cache_optimized,
        }
    }

    pub fn validate_legacy_format_profile(&self) -> Result<()> {
        let report = self.legacy_format_report();

        match report.allocation_unit_bytes {
            Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES) => Ok(()),
            Some(actual) => Err(anyhow!(
                "Perfil FAT32 no optimizado para firmware legacy en '{}': allocation unit detectado {} KB, requerido {} KB. Formatee despues del backup con FAT32 y cluster de 32 KB.",
                self.mount_point.display(),
                actual / 1024,
                LEGACY_FAT32_ALLOCATION_UNIT_BYTES / 1024,
            )),
            None => {
                warn!(
                    "No se pudo determinar el allocation unit FAT32 de '{}'. Se continua en modo best-effort sin forzar reformateo.",
                    self.mount_point.display()
                );
                Ok(())
            }
        }
    }

    pub fn requires_legacy_reformat(&self) -> bool {
        (self.fs_type != "vfat" && self.fs_type != "fat32")
            || matches!(
                self.allocation_unit_bytes,
                Some(value) if value != LEGACY_FAT32_ALLOCATION_UNIT_BYTES
            )
    }
}

fn push_identity_part(parts: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
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

fn read_trimmed_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

fn read_first_available(paths: &[PathBuf]) -> Option<String> {
    paths.iter().find_map(|path| read_trimmed_file(path))
}

fn lookup_device_alias(alias_dir: &Path, device_path: &Path) -> Option<String> {
    let canonical_device = fs::canonicalize(device_path).ok()?;
    let entries = fs::read_dir(alias_dir).ok()?;

    for entry in entries.flatten() {
        let alias_path = entry.path();
        if let Ok(target) = fs::canonicalize(&alias_path) {
            if target == canonical_device {
                return entry.file_name().into_string().ok();
            }
        }
    }

    None
}

#[derive(Debug, Clone, Default)]
struct DeviceIdentity {
    vendor: Option<String>,
    model: Option<String>,
    serial: Option<String>,
    fs_uuid: Option<String>,
    fs_label: Option<String>,
}

fn read_device_identity(device_path: &Path, device_name: &str) -> DeviceIdentity {
    let parent = get_parent_block_device(device_name);

    let vendor = read_first_available(&[PathBuf::from(format!(
        "/sys/block/{}/device/vendor",
        parent
    ))]);
    let model = read_first_available(&[PathBuf::from(format!(
        "/sys/block/{}/device/model",
        parent
    ))]);
    let serial = read_first_available(&[
        PathBuf::from(format!("/sys/block/{}/device/serial", parent)),
        PathBuf::from(format!("/sys/block/{}/serial", parent)),
    ]);
    let fs_uuid = lookup_device_alias(Path::new("/dev/disk/by-uuid"), device_path);
    let fs_label = lookup_device_alias(Path::new("/dev/disk/by-label"), device_path);

    DeviceIdentity {
        vendor,
        model,
        serial,
        fs_uuid,
        fs_label,
    }
}

fn read_fat32_allocation_unit_bytes(device_path: &Path) -> Option<u64> {
    let mut file = File::open(device_path).ok()?;
    let mut boot_sector = [0u8; 512];
    file.read_exact(&mut boot_sector).ok()?;

    if boot_sector[510] != 0x55 || boot_sector[511] != 0xAA {
        return None;
    }

    let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]) as u64;
    let sectors_per_cluster = boot_sector[13] as u64;

    if bytes_per_sector == 0 || sectors_per_cluster == 0 {
        return None;
    }

    Some(bytes_per_sector.saturating_mul(sectors_per_cluster))
}

fn read_logical_sector_bytes(device_path: &Path) -> Option<u64> {
    let device_name = device_path.file_name()?.to_str()?;
    let parent = get_parent_block_device(device_name);
    let sysfs_path = PathBuf::from(format!(
        "/sys/block/{}/queue/logical_block_size",
        parent
    ));

    read_trimmed_file(&sysfs_path)?.parse::<u64>().ok()
}

fn sectors_per_cluster_for_legacy(logical_sector_bytes: u64) -> Option<u64> {
    if logical_sector_bytes == 0
        || !LEGACY_FAT32_ALLOCATION_UNIT_BYTES.is_multiple_of(logical_sector_bytes)
    {
        return None;
    }

    Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES / logical_sector_bytes)
}

fn sanitize_fat_label(label: Option<&str>) -> Option<String> {
    let raw = label?.trim();
    if raw.is_empty() {
        return None;
    }

    let sanitized: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .map(|ch| ch.to_ascii_uppercase())
        .take(11)
        .collect();

    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

fn run_mkfs_fat32(device_path: &Path, sectors_per_cluster: u64, label: Option<&str>) -> std::result::Result<(), ProvisioningError> {
    let label = sanitize_fat_label(label);

    for binary in ["mkfs.vfat", "mkfs.fat"] {
        let mut command = Command::new(binary);
        command
            .arg("-F")
            .arg("32")
            .arg("-s")
            .arg(sectors_per_cluster.to_string());

        if let Some(label) = &label {
            command.arg("-n").arg(label);
        }

        command.arg(device_path);

        match command.status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                return Err(ProvisioningError::ProvisioningFailed {
                    details: format!(
                        "El formateo FAT32 fallo sobre '{}' con {} (exit code {:?}).",
                        device_path.display(),
                        binary,
                        status.code()
                    ),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(ProvisioningError::ProvisioningFailed {
                    details: format!(
                        "No se pudo ejecutar {} sobre '{}': {}",
                        binary,
                        device_path.display(),
                        e
                    ),
                });
            }
        }
    }

    Err(ProvisioningError::ProvisioningFailed {
        details: "No se encontro mkfs.vfat ni mkfs.fat en el PATH del host.".to_string(),
    })
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
        let identity = read_device_identity(Path::new(device_path), device_name);
        let allocation_unit_bytes = read_fat32_allocation_unit_bytes(Path::new(device_path));

        devices.push(DeviceInfo {
            device_path: PathBuf::from(device_path),
            mount_point: PathBuf::from(mount_point),
            fs_type: fs_type.to_string(),
            total_bytes,
            available_bytes,
            is_removable,
            vendor: identity.vendor,
            model: identity.model,
            serial: identity.serial,
            fs_uuid: identity.fs_uuid,
            fs_label: identity.fs_label,
            allocation_unit_bytes,
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
    let device = inspect_device_path(target_mount_point)?;

    device.is_valid_for_provisioning()?;
    device.validate_legacy_format_profile()?;

    if device.requires_confirmation() {
        warn!(
            "ALERTA: El dispositivo '{}' tiene un tamano de {:.1} GB. Asegurese de que es la USB correcta.",
            device.mount_point.display(),
            device.size_gb()
        );
    }

    Ok(device)
}

pub fn inspect_device_path(target_mount_point: &Path) -> Result<DeviceInfo> {
    let target_str = target_mount_point
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();

    let all_devices = get_mounted_devices()?;

    let matched_device = all_devices
        .into_iter()
        .find(|d| d.mount_point.to_string_lossy().trim_end_matches('/') == target_str);

    match matched_device {
        Some(device) => Ok(device),
        None => Err(anyhow!(
            "Ruta denegada: '{}' no corresponde a ningun dispositivo de bloque montado en /proc/mounts. No intente ejecutar el aprovisionador sobre directorios locales.",
            target_str
        )),
    }
}

#[cfg(target_os = "linux")]
pub fn format_device_for_legacy(
    device: &DeviceInfo,
    mount_point: &Path,
    label: Option<&str>,
) -> std::result::Result<(), ProvisioningError> {
    if !device.is_removable {
        return Err(ProvisioningError::InvalidConfig {
            details: format!(
                "El dispositivo '{}' no es removible; formateo abortado.",
                device.device_path.display()
            ),
        });
    }

    let logical_sector_bytes = read_logical_sector_bytes(&device.device_path).unwrap_or(512);
    let Some(sectors_per_cluster) = sectors_per_cluster_for_legacy(logical_sector_bytes) else {
        return Err(ProvisioningError::ProvisioningFailed {
            details: format!(
                "No se pudo calcular sectors-per-cluster para '{}' con sector logico {} bytes.",
                device.device_path.display(),
                logical_sector_bytes
            ),
        });
    };

    Command::new("sync").status().map_err(|e| ProvisioningError::ProvisioningFailed {
        details: format!("No se pudo ejecutar sync antes del formateo: {}", e),
    })?;

    let umount_status = Command::new("umount")
        .arg(mount_point)
        .status()
        .map_err(|e| ProvisioningError::ProvisioningFailed {
            details: format!(
                "No se pudo desmontar '{}' antes del formateo: {}",
                mount_point.display(),
                e
            ),
        })?;

    if !umount_status.success() {
        return Err(ProvisioningError::ProvisioningFailed {
            details: format!(
                "umount fallo para '{}' (exit code {:?}).",
                mount_point.display(),
                umount_status.code()
            ),
        });
    }

    run_mkfs_fat32(&device.device_path, sectors_per_cluster, label.or(device.fs_label.as_deref()))?;

    fs::create_dir_all(mount_point).map_err(|e| ProvisioningError::ProvisioningFailed {
        details: format!(
            "No se pudo preparar el mountpoint '{}' tras el formateo: {}",
            mount_point.display(),
            e
        ),
    })?;

    let mount_status = Command::new("mount")
        .arg(&device.device_path)
        .arg(mount_point)
        .status()
        .map_err(|e| ProvisioningError::ProvisioningFailed {
            details: format!(
                "No se pudo remontar '{}' en '{}': {}",
                device.device_path.display(),
                mount_point.display(),
                e
            ),
        })?;

    if !mount_status.success() {
        return Err(ProvisioningError::ProvisioningFailed {
            details: format!(
                "mount fallo para '{}' en '{}' (exit code {:?}).",
                device.device_path.display(),
                mount_point.display(),
                mount_status.code()
            ),
        });
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn format_device_for_legacy(
    device: &DeviceInfo,
    _mount_point: &Path,
    _label: Option<&str>,
) -> std::result::Result<(), ProvisioningError> {
    Err(ProvisioningError::ProvisioningFailed {
        details: format!(
            "El formateo automatico legacy no esta soportado en este host para '{}'.",
            device.device_path.display()
        ),
    })
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
    /// # Retorna
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
        fs::write(&lock_path, my_pid.to_string()).map_err(|e| {
            ProvisioningError::ProvisioningFailed {
                details: format!(
                    "Fallo la escritura del archivo de bloqueo de concurrencia en {}: {}",
                    lock_path.display(),
                    e
                ),
            }
        })?;

        info!("Hardware Lock adquirido con éxito (PID: {})", my_pid);

        Ok(Self { lock_path })
    }
}

/// [R-02-009] Chequeo de Salud S.M.A.R.T. Lite (Hardware RO Lock)
/// Referencia legacy: R-13.
/// Precondición: `device_path` corresponde a un dispositivo de bloques válido en `/dev/`.
/// Postcondición: retorna `Ok(())` si el flag de hardware `ro` en SysFS es 0, o si SysFS no está disponible.
/// Invariante: el provisionador debe abortar antes de cualquier operación si el controlador NAND se bloqueó a sí mismo por agotamiento de vida útil.
pub fn assert_hardware_health(device_path: &Path) -> std::result::Result<(), ProvisioningError> {
    let device_name = device_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ProvisioningError::InvalidConfig {
            details: format!("Ruta de dispositivo inválida: {}", device_path.display()),
        })?;

    let parent_block = get_parent_block_device(device_name);
    let ro_flag_path = format!("/sys/block/{}/ro", parent_block);

    match std::fs::read_to_string(&ro_flag_path) {
        Ok(content) => {
            if content.trim() == "1" {
                return Err(ProvisioningError::HardwareFraudDetected {
                    details: format!(
                        "NAND Wear-Leveling Exhausto: El controlador de hardware bloqueó '{}' a modo solo-lectura. La memoria ha llegado al fin de su vida útil y desecharse.",
                        device_path.display()
                    ),
                });
            }
            Ok(())
        }
        Err(e) => {
            log::warn!(
                "No se pudo consultar la salud hardware en {} (sonda SMART Lite omitida): {}",
                ro_flag_path,
                e
            );
            // Degradación elegante: Si el host no expone SysFS (macOS/Windows) o falta acceso,
            // se permite continuar y se confía en el dirty-bit test (assert_rw_filesystem).
            Ok(())
        }
    }
}

/// [R-02-002] Dirty Bit Test
/// Legacy cross-ref: R-20.
/// Pre-condition: `usb_mount` apunta a un mountpoint ya validado como destino operativo.
/// Post-condition: retorna `Ok(())` solo si el volumen acepta una escritura real y limpieza del probe temporal.
/// Invariant: un volumen en solo lectura nunca avanza al pipeline de provisioning.
///
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

/// [R-02-005] FAT32 Dirty Bit Early Detection
/// Referencia legacy: R-18 (Chequeo pre-flight de Fase 2).
/// Precondición: `usb_mount` ya fue seleccionado como candidato de provisioning y aún no se han iniciado escrituras de negocio.
/// Postcondición: aborta antes de cualquier mutación del pipeline si el probe `.fat32_dirty_test` falla.
/// Invariante: el pre-flight usa el mismo probe físico que la validación RW y nunca deja artefactos persistentes tras una ruta exitosa.
pub fn run_preflight_rw_probe(usb_mount: &Path) -> std::result::Result<(), ProvisioningError> {
    assert_rw_filesystem(usb_mount)
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
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
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
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
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
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
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
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
        };

        assert!(device.is_valid_for_provisioning().is_err());
    }

    #[test]
    fn test_legacy_format_profile_accepts_32kb_allocation_unit() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            available_bytes: 8 * 1024 * 1024 * 1024,
            is_removable: true,
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
        };

        assert!(device.validate_legacy_format_profile().is_ok());
        assert!(device.legacy_format_report().is_legacy_cache_optimized);
    }

    #[test]
    fn test_legacy_format_profile_rejects_non_32kb_allocation_unit() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            available_bytes: 8 * 1024 * 1024 * 1024,
            is_removable: true,
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(4 * 1024),
        };

        let err = device.validate_legacy_format_profile().unwrap_err();
        assert!(err.to_string().contains("allocation unit detectado 4 KB"));
    }

    #[test]
    fn test_legacy_format_profile_allows_unknown_allocation_unit() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            available_bytes: 8 * 1024 * 1024 * 1024,
            is_removable: true,
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: None,
        };

        assert!(device.validate_legacy_format_profile().is_ok());
        assert!(!device.legacy_format_report().is_legacy_cache_optimized);
    }

    #[test]
    fn test_requires_legacy_reformat_only_for_non_compliant_profiles() {
        let compliant = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            available_bytes: 8 * 1024 * 1024 * 1024,
            is_removable: true,
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
        };
        let unknown = DeviceInfo {
            allocation_unit_bytes: None,
            ..compliant.clone()
        };
        let wrong_cluster = DeviceInfo {
            allocation_unit_bytes: Some(4096),
            ..compliant.clone()
        };
        let wrong_fs = DeviceInfo {
            fs_type: "ntfs".to_string(),
            ..compliant
        };

        assert!(!unknown.requires_legacy_reformat());
        assert!(!DeviceInfo { fs_type: "vfat".to_string(), allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES), ..unknown.clone() }.requires_legacy_reformat());
        assert!(wrong_cluster.requires_legacy_reformat());
        assert!(wrong_fs.requires_legacy_reformat());
    }

    #[test]
    fn test_sectors_per_cluster_for_legacy() {
        assert_eq!(sectors_per_cluster_for_legacy(512), Some(64));
        assert_eq!(sectors_per_cluster_for_legacy(4096), Some(8));
        assert_eq!(sectors_per_cluster_for_legacy(1000), None);
    }

    #[test]
    fn test_sanitize_fat_label() {
        assert_eq!(sanitize_fat_label(Some("cabina a 01")), Some("CABINAA01".to_string()));
        assert_eq!(sanitize_fat_label(Some("legacy_audio_usb")), Some("LEGACY_AUDI".to_string()));
        assert_eq!(sanitize_fat_label(Some("***")), None);
    }

    #[test]
    fn test_backup_identity_key_prefers_device_metadata() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            available_bytes: 8 * 1024 * 1024 * 1024,
            is_removable: true,
            vendor: Some("SanDisk".to_string()),
            model: Some("Ultra Fit".to_string()),
            serial: Some("4C530001230101117391".to_string()),
            fs_uuid: Some("ABCD-1234".to_string()),
            fs_label: Some("CABINA_A".to_string()),
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
        };

        assert_eq!(
            device.backup_identity_key(),
            "CABINA_A__SanDisk__Ultra Fit__4C530001230101117391__ABCD-1234"
        );
    }

    #[test]
    fn test_backup_identity_key_falls_back_when_metadata_missing() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/sdb1"),
            mount_point: PathBuf::from("/media/user/DISK"),
            fs_type: "vfat".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            available_bytes: 8 * 1024 * 1024 * 1024,
            is_removable: true,
            vendor: None,
            model: None,
            serial: None,
            fs_uuid: None,
            fs_label: None,
            allocation_unit_bytes: Some(LEGACY_FAT32_ALLOCATION_UNIT_BYTES),
        };

        assert_eq!(
            device.backup_identity_key(),
            format!("{}__{}", device.device_path.display(), device.total_bytes)
        );
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
