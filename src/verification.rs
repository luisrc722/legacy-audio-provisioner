//! R-T5: Verificacion final y expulsion segura
//!
//! Implementacion de caja blanca para auditar las invariantes del hardware legacy:
//! - FAT32 Limits: maximo 50 archivos por volumen, nombres <= 32 caracteres.
//! - Topologia: profundidad estricta de 2 niveles (Raiz -> VOL_XX -> Archivo).
//! - Integridad Criptografica: validacion de hashes contra el estado atomico.
//! - Expulsion: volcado de cache (sync) obligatorio antes de desmontar.

use crate::checkpoint::{CheckpointData, OperationStatus};
use crate::error::ProvisioningError;
use anyhow::{anyhow, Result};
use log::{error, info, warn};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::Command;

fn is_valid_sha256_hex(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Default)]
pub struct VerificationReport {
    pub total_volumes: usize,
    pub total_files: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub success: bool,
}

impl VerificationReport {
    pub fn new() -> Self {
        VerificationReport {
            success: true,
            ..Default::default()
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
        self.success = false;
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn print_summary(&self) {
        let json_mode = std::env::var("LAP_JSON_MODE").unwrap_or_default() == "1";

        if json_mode {
            eprintln!("\n=== Final Verification Report ===");
            eprintln!("Volumes Verified: {}", self.total_volumes);
            eprintln!("Files Verified: {}", self.total_files);
        } else {
            println!("\n=== Final Verification Report ===");
            println!("Volumes Verified: {}", self.total_volumes);
            println!("Files Verified: {}", self.total_files);
        }

        if !self.warnings.is_empty() {
            if json_mode {
                eprintln!("\nWarnings ({}):", self.warnings.len());
            } else {
                println!("\nWarnings ({}):", self.warnings.len());
            }
            for w in &self.warnings {
                if json_mode {
                    eprintln!("  [!] {}", w);
                } else {
                    println!("  [!] {}", w);
                }
            }
        }

        if !self.errors.is_empty() {
            if json_mode {
                eprintln!("\nErrors ({}):", self.errors.len());
            } else {
                println!("\nErrors ({}):", self.errors.len());
            }
            for e in &self.errors {
                if json_mode {
                    eprintln!("  [X] {}", e);
                } else {
                    println!("  [X] {}", e);
                }
            }
        }

        if self.success {
            if json_mode {
                eprintln!("\nOK Hardware Invariants PASSED");
            } else {
                println!("\nOK Hardware Invariants PASSED");
            }
        } else if json_mode {
            eprintln!("\nFAIL Verification FAILED - DO NOT USE THIS USB");
        } else {
            println!("\nFAIL Verification FAILED - DO NOT USE THIS USB");
        }
    }
}

/// Verifica la topologia estricta del sistema de archivos en la USB.
pub fn verify_directory_structure(usb_mount_point: &Path) -> Result<VerificationReport> {
    let mut report = VerificationReport::new();
    info!(
        "Auditing hardware directory constraints at {}",
        usb_mount_point.display()
    );

    if !usb_mount_point.exists() {
        report.add_error(format!(
            "Mount point inaccessible: {}",
            usb_mount_point.display()
        ));
        return Ok(report);
    }

    for entry in fs::read_dir(usb_mount_point)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Ignora ruido de sistema comun en la raiz del volumen.
        if file_name.starts_with('.') || file_name.starts_with("System Volume") {
            continue;
        }

        if !path.is_dir() {
            report.add_warning(format!(
                "Orphan file found in root (will be ignored by stereo): {}",
                file_name
            ));
            continue;
        }

        if !file_name.starts_with("VOL_") {
            report.add_error(format!("Invalid volume folder format: {}", file_name));
            continue;
        }

        report.total_volumes += 1;
        let mut file_count = 0;

        for sub_entry in fs::read_dir(&path)? {
            let sub_entry = sub_entry?;
            let sub_path = sub_entry.path();
            let sub_name = sub_entry.file_name().to_string_lossy().to_string();

            if sub_path.is_dir() {
                report.add_error(format!(
                    "Illegal nesting level detected: {}",
                    sub_path.display()
                ));
                continue;
            }

            if sub_name.len() > 32 {
                report.add_error(format!("Hardware limit exceeded (>32 chars): {}", sub_name));
            }

            if !sub_name.is_ascii() {
                report.add_error(format!("Non-ASCII characters detected in: {}", sub_name));
            }

            file_count += 1;
        }

        if file_count > 50 {
            report.add_error(format!(
                "FAT32 Buffer Overflow risk: {} contains {} files (Max 50)",
                file_name, file_count
            ));
        }

        report.total_files += file_count;
    }

    report.success = report.errors.is_empty();
    Ok(report)
}

/// Verifica la consistencia criptografica consumiendo la bitacora atomica.
pub fn verify_file_integrity(
    usb_mount_point: &Path,
    checkpoint: &CheckpointData,
) -> Result<VerificationReport> {
    let mut report = VerificationReport::new();
    info!("Running cryptographic integrity check...");
    let mut consecutive_hash_failures = 0usize;
    let fraud_threshold = 5usize;

    for (index, file_data) in &checkpoint.processed_files {
        if file_data.status != OperationStatus::Completed {
            continue;
        }

        let volume_index = (index / 50) + 1;
        let target_path = usb_mount_point
            .join(format!("VOL_{:02}", volume_index))
            .join(&file_data.normalized_name);

        if !target_path.exists() {
            report.add_error(format!(
                "Missing file post-provisioning: {}",
                target_path.display()
            ));
            continue;
        }

        report.total_files += 1;

        match &file_data.usb_checksum {
            Some(expected_hash) if is_valid_sha256_hex(expected_hash) => {
                let mut file = File::open(&target_path)?;
                let mut hasher = Sha256::new();
                let mut buffer = [0; 65536];

                loop {
                    let bytes_read = file.read(&mut buffer)?;
                    if bytes_read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..bytes_read]);
                }

                let actual_hash = hex::encode(hasher.finalize());
                if actual_hash != *expected_hash {
                    consecutive_hash_failures += 1;
                    error!("Hash mismatch detected on {}", target_path.display());
                    report.add_error(format!("SHA256 Mismatch: {}", target_path.display()));

                    if consecutive_hash_failures >= fraud_threshold {
                        return Err(anyhow::Error::new(
                            ProvisioningError::HardwareFraudDetected {
                                details: format!(
                                    "Se detectaron {} fallos criptograficos consecutivos. El controlador reporta espacio falso y sobreescribe datos.",
                                    consecutive_hash_failures
                                ),
                            },
                        ));
                    }
                } else {
                    consecutive_hash_failures = 0;
                }
            }
            Some(_) => {
                report.add_error(format!(
                    "Invalid checkpoint hash format for {}",
                    target_path.display()
                ));
            }
            None => {
                report.add_error(format!(
                    "Missing checkpoint hash for {}",
                    target_path.display()
                ));
            }
        }
    }

    report.success = report.errors.is_empty();
    Ok(report)
}

/// Ejecuta la suite completa de auditoria.
pub fn pre_eject_verification(
    usb_mount_point: &Path,
    checkpoint: &CheckpointData,
) -> Result<VerificationReport> {
    info!("Initiating pre-eject QA sequence...");

    let mut report = verify_directory_structure(usb_mount_point)?;

    if report.success {
        let integrity = verify_file_integrity(usb_mount_point, checkpoint)?;
        report.total_files = report.total_files.max(integrity.total_files);
        report.errors.extend(integrity.errors);
        report.warnings.extend(integrity.warnings);
        report.success = report.errors.is_empty();
    }

    report.print_summary();
    Ok(report)
}

/// Fuerza el volcado a disco y desmonta de manera segura.
#[cfg(target_os = "linux")]
pub fn safe_eject(device_path: &Path, mount_point: &Path) -> Result<()> {
    info!("Flushing OS buffers and ejecting {}", device_path.display());

    Command::new("sync").status()?;

    let umount = Command::new("umount").arg(mount_point).status()?;
    if !umount.success() {
        warn!("umount command failed, device might be busy.");
    }

    let eject = Command::new("udisksctl")
        .args(["power-off", "-b", &device_path.to_string_lossy()])
        .status()?;

    if eject.success() {
        info!("OK Device safely powered off.");
        Ok(())
    } else {
        Err(anyhow!("udisksctl failed to power off the block device."))
    }
}

#[cfg(not(target_os = "linux"))]
pub fn safe_eject(device_path: &Path, _mount_point: &Path) -> Result<()> {
    warn!(
        "Automated safe eject is only supported on Linux. Please eject {} manually.",
        device_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_report_creation() {
        let report = VerificationReport::new();
        assert!(report.success);
        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_verification_report_with_errors() {
        let mut report = VerificationReport::new();
        report.add_error("Test error".to_string());
        assert!(!report.success);
        assert_eq!(report.errors.len(), 1);
    }
}
