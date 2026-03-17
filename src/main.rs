use anyhow::{Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

// Consumimos nuestra propia libreria (definida en src/lib.rs)
use legacy_audio_provisioner::error::ProvisioningError;
use legacy_audio_provisioner::ipc::IpcEvent;
use legacy_audio_provisioner::{
    audio_discovery, backup, checkpoint, diffing, distribution, hardware, normalizer, recovery,
    sanitizer, verification,
};

fn append_drm_skip_log(backup_dir: &std::path::Path, original_path: &std::path::Path) {
    let log_path = backup_dir.join("unsupported_drm_files.log");
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(file, "{}", original_path.display());
    }
}

fn human_out(json_mode: bool, message: &str) {
    if !json_mode {
        println!("{}", message);
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "Legacy Audio Provisioner",
    version = env!("CARGO_PKG_VERSION"),
    author = "Spec-Driven Development",
    about = "Prepare USB drives for legacy audio systems",
    long_about = "Transforms and normalizes audio files for compatibility with \
                   legacy audio systems (32-bit firmware, FAT32, strict naming conventions)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[arg(
        long,
        help = "Emite eventos IPC en formato JSON Lines por stdout",
        global = true
    )]
    json: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Lista los dispositivos USB/extraibles detectados
    List,

    /// Escanea la primera USB detectada en busca de archivos de audio
    Scan,

    /// Procesa, normaliza y sincroniza audio hacia la USB
    Provision {
        #[arg(short, long, value_name = "PATH")]
        usb_mount: PathBuf,

        #[arg(short, long, value_name = "PATH")]
        audio_source: PathBuf,

        #[arg(long)]
        dry_run: bool,

        #[arg(long, help = "Modo incremental: solo procesa archivos nuevos por hash")]
        sync: bool,
    },

    /// Reanuda una sesion interrumpida desde un backup
    Resume {
        #[arg(short, long, value_name = "PATH")]
        usb_mount: PathBuf,

        #[arg(long, value_name = "BACKUP_DIR")]
        resume: PathBuf,
    },
}

fn main() -> std::result::Result<(), ProvisioningError> {
    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    std::env::set_var("RUST_LOG", log_level);
    env_logger::init();

    info!("=== Legacy Audio Provisioner ===");
    info!(
        "Version {} | Spec-Driven Development",
        env!("CARGO_PKG_VERSION")
    );

    let execution_result: std::result::Result<(), ProvisioningError> = match cli.command {
        Commands::List => {
            if cli.json {
                Err(ProvisioningError::UnsupportedJsonMode {
                    feature: "list".to_string(),
                })
            } else {
                list_usb_devices().map_err(ProvisioningError::from_anyhow)
            }
        }
        Commands::Scan => {
            if cli.json {
                Err(ProvisioningError::UnsupportedJsonMode {
                    feature: "scan".to_string(),
                })
            } else {
                scan_usb_audio_automatically().map_err(ProvisioningError::from_anyhow)
            }
        }
        Commands::Resume { usb_mount, resume } => {
            resume_provisioning(&resume, &usb_mount, cli.json)
                .map_err(ProvisioningError::from_anyhow)
        }
        Commands::Provision {
            usb_mount,
            audio_source,
            dry_run,
            sync,
        } => {
            validate_canonical_paths(&usb_mount, &audio_source)?;
            provision_usb(&usb_mount, &audio_source, dry_run, sync, cli.json)
                .map_err(ProvisioningError::from_anyhow)
        }
    };

    if let Err(e) = execution_result {
        IpcEvent::FatalError {
            code: e.code().to_string(),
            message: e.to_string(),
            action_required: e.action_required().to_string(),
        }
        .emit(cli.json);
        return Err(e);
    }

    Ok(())
}

fn list_usb_devices() -> Result<()> {
    println!("\n=== Detecting USB Devices ===\n");
    let devices = hardware::detect_usb_devices()?;
    if devices.is_empty() {
        println!("❌ No USB/removable devices detected.");
        return Ok(());
    }
    println!("✅ Found {} USB device(s):\n", devices.len());
    for (idx, device) in devices.iter().enumerate() {
        println!("  [{}] Device: {}", idx + 1, device.device_path.display());
        println!("      Mount point: {}", device.mount_point.display());
        println!("      Filesystem: {}", device.fs_type);
        println!("      Size: {:.2} GB", device.size_gb());
        println!(
            "      Removable: {}",
            if device.is_removable {
                "✓ Yes"
            } else {
                "✗ No"
            }
        );
        println!();
    }
    Ok(())
}

fn scan_usb_audio_automatically() -> Result<()> {
    let devices = hardware::detect_usb_devices()?;
    if devices.is_empty() {
        eprintln!("\n❌ ERROR: No USB devices detected.");
        return Ok(());
    }
    let device = &devices[0];
    println!(
        "\n🔍 Scanning for audio files on {}...",
        device.mount_point.display()
    );

    let report = audio_discovery::discover_audio_files(&device.mount_point)?;

    if report.audio_files.is_empty() {
        println!("❌ No audio files found.");
        return Ok(());
    }

    println!("✅ Found {} audio file(s)", report.audio_files.len());
    println!("  Total size: {:.2} MB", report.total_size_mb());
    Ok(())
}

fn validate_canonical_paths(
    usb_mount: &std::path::Path,
    audio_source: &std::path::Path,
) -> std::result::Result<(), ProvisioningError> {
    let usb_can = usb_mount
        .canonicalize()
        .map_err(|e| ProvisioningError::InvalidConfig {
            details: format!("No se pudo resolver la ruta USB '{}': {}", usb_mount.display(), e),
        })?;

    let src_can = audio_source
        .canonicalize()
        .map_err(|e| ProvisioningError::InvalidConfig {
            details: format!(
                "No se pudo resolver la ruta de origen '{}': {}",
                audio_source.display(),
                e
            ),
        })?;

    if usb_can == src_can {
        return Err(ProvisioningError::InvalidConfig {
            details: format!(
                "El origen y el destino son la misma ubicacion fisica: '{}'",
                usb_can.display()
            ),
        });
    }

    if src_can.starts_with(&usb_can) {
        return Err(ProvisioningError::InvalidConfig {
            details: format!(
                "El origen de audio '{}' no puede estar dentro de la USB de destino '{}'.",
                src_can.display(),
                usb_can.display()
            ),
        });
    }

    Ok(())
}

fn provision_usb(
    usb_mount: &std::path::Path,
    audio_source: &std::path::Path,
    dry_run: bool,
    sync_mode: bool,
    json_mode: bool,
) -> Result<()> {
    let start = Instant::now();
    human_out(json_mode, "\n=== Starting USB Provisioning ===");
    if dry_run {
        human_out(json_mode, "[DRY RUN] No actual changes will be made\n");
    }

    human_out(json_mode, "Step 1: Validating USB device...");
    let device = hardware::validate_device_path(usb_mount)?;
    device.is_valid_for_provisioning()?;

    // R-18: INYECCIÓN del Lock de exclusión mutua.
    // Usamos el prefijo `_lock` para indicar al compilador que mantenga la variable viva
    // pero sin generar warnings de "variable no usada".
    let _lock = hardware::ProvisioningLock::acquire(usb_mount)?;

    // R-20: Validacion de I/O fisica (Dirty Bit Test)
    hardware::assert_rw_filesystem(usb_mount)?;

    human_out(
        json_mode,
        &format!(
            "USB device validated (RW & Locked): {}",
            usb_mount.display()
        ),
    );

    human_out(json_mode, "\nStep 2: Scanning audio files (Secure Mode)...");
    let discovery_report = audio_discovery::discover_audio_files(audio_source)?;
    let discovered_source_files = discovery_report.audio_files;
    human_out(
        json_mode,
        &format!("Found {} audio files", discovered_source_files.len()),
    );

    if discovered_source_files.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid audio files found in source. Aborting."
        ));
    }

    let mut next_global_index = 1usize;
    let mut existing_volume_counts = std::collections::BTreeMap::new();
    let mut checkpoint_known_names: HashSet<String> = HashSet::new();

    if sync_mode {
        human_out(
            json_mode,
            "Step 2.1: Incremental sync mode enabled (USB hash diff)...",
        );

        let usb_checkpoint_path = usb_mount.join(".provisioning_checkpoint");
        if usb_checkpoint_path.exists() {
            if let Ok(usb_checkpoint_mgr) =
                checkpoint::CheckpointManager::load_from_disk(&usb_checkpoint_path)
            {
                for file_cp in usb_checkpoint_mgr.get_data().processed_files.values() {
                    checkpoint_known_names.insert(file_cp.normalized_name.clone());
                    if let Some((prefix, _)) = file_cp.normalized_name.split_once('_') {
                        if let Ok(parsed) = prefix.parse::<usize>() {
                            next_global_index = next_global_index.max(parsed.saturating_add(1));
                        }
                    }
                }
                human_out(
                    json_mode,
                    &format!(
                        "Checkpoint USB cargado: {} entradas conocidas",
                        checkpoint_known_names.len()
                    ),
                );
            }
        }
    }

    let (audio_files, skipped_existing, untracked_in_target) = if sync_mode {
        let diff_report = diffing::calculate_sync_diff(
            &discovered_source_files,
            usb_mount,
            &checkpoint_known_names,
        )?;

        next_global_index = next_global_index.max(diff_report.max_existing_index.saturating_add(1));
        existing_volume_counts = diff_report.existing_volume_counts;

        (
            diff_report.files_to_process,
            diff_report.skipped_existing,
            diff_report.untracked_in_target,
        )
    } else {
        (discovered_source_files, 0usize, Vec::new())
    };

    if sync_mode {
        human_out(
            json_mode,
            &format!(
                "Diff completo: {} nuevos, {} ya existentes (skip), {} untracked en USB",
                audio_files.len(),
                skipped_existing,
                untracked_in_target.len()
            ),
        );
    }

    human_out(json_mode, "\nStep 3: Validating backup capacity...");
    let backup_home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let backup_home_path = std::path::Path::new(&backup_home);
    let new_audio_size: u64 = audio_files.iter().map(|f| f.size_bytes).sum();
    let untracked_backup_size: u64 = untracked_in_target
        .iter()
        .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    let total_size = new_audio_size.saturating_add(untracked_backup_size);

    backup::check_disk_space(total_size, backup_home_path)?;

    let mut backup = if dry_run {
        None
    } else {
        Some(backup::BackupMetadata::new(backup_home_path)?)
    };

    let mut quarantined_count = 0usize;

    if let Some(backup_meta) = backup.as_mut() {
        for file in &audio_files {
            backup_meta.backup_file(&file.path)?;
        }

        if sync_mode && !untracked_in_target.is_empty() {
            human_out(
                json_mode,
                "Step 3.1: Aislando huérfanos en .legacy_quarantine (backup-first)...",
            );
            let session_label = format!("sync_{}", Local::now().format("%Y%m%d_%H%M%S"));
            let quarantine_report = diffing::quarantine_untracked_files(
                usb_mount,
                &untracked_in_target,
                backup_meta,
                &session_label,
            )?;

            quarantined_count = quarantine_report.quarantined.len();

            if !quarantine_report.failed.is_empty() {
                for (path, details) in quarantine_report.failed.iter().take(5) {
                    IpcEvent::Warning {
                        code: "UNTRACKED_QUARANTINE_FAILED".to_string(),
                        source_file: path.display().to_string(),
                        message: details.clone(),
                    }
                    .emit(json_mode);
                }
            }

            human_out(
                json_mode,
                &format!(
                    "Cuarentena completada: {} movidos, {} fallidos",
                    quarantine_report.quarantined.len(),
                    quarantine_report.failed.len()
                ),
            );
        }

        if !backup_meta.verify_backup()? {
            return Err(anyhow::anyhow!(
                "❌ Backup verification failed - corrupted files detected"
            ));
        }
        human_out(
            json_mode,
            &format!(
                "Backup verified. Directory: {}",
                backup_meta.backup_dir.display()
            ),
        );
    }

    if audio_files.is_empty() {
        human_out(
            json_mode,
            "No hay archivos nuevos para procesar en modo --sync.",
        );
        IpcEvent::Success {
            total_processed: 0,
            total_skipped: skipped_existing,
            elapsed_time_seconds: start.elapsed().as_secs(),
            message: format!(
                "Sincronizacion completada: sin cambios nuevos; {} archivo(s) aislado(s) en .legacy_quarantine.",
                quarantined_count
            ),
        }
        .emit(json_mode);
        return Ok(());
    }

    human_out(
        json_mode,
        "\nStep 4: Forcing MP3 extension, Sanitizing & Initializing Checkpoint...",
    );
    let checkpoint_backup_dir = backup
        .as_ref()
        .map(|b| b.backup_dir.clone())
        .unwrap_or_else(|| backup_home_path.join("dry_run_no_backup"));
    let mut checkpoint = checkpoint::CheckpointManager::new(
        checkpoint_backup_dir,
        usb_mount.to_path_buf(),
        audio_source.to_path_buf(),
        audio_files.len(),
    )?;

    let file_mappings: Vec<(PathBuf, String)> = audio_files
        .iter()
        .enumerate()
        .map(|(idx, file)| {
            // FORZADO DE EXTENSION: Todo destino en la USB debe ser MP3
            let stem = file.path.file_stem().unwrap().to_string_lossy();
            let forced_mp3_name = format!("{}.mp3", stem);

            let sanitized = sanitizer::sanitize_filename(&forced_mp3_name);
            let indexed = sanitizer::add_sequential_prefix(&sanitized, idx + next_global_index);
            (file.path.clone(), indexed)
        })
        .collect();

    let volumes = if sync_mode {
        diffing::plan_incremental_distribution(file_mappings, &existing_volume_counts)
    } else {
        distribution::plan_distribution(file_mappings)?
    };
    human_out(json_mode, &format!("Planned {} volume(s)", volumes.len()));

    if !dry_run {
        human_out(
            json_mode,
            "\nStep 5: Executing Physical Normalization & Copy...",
        );

        let mut global_idx = 0;
        let mut skipped_drm = 0usize;
        let pb = if json_mode {
            ProgressBar::hidden()
        } else {
            ProgressBar::new(audio_files.len() as u64)
        };
        if !json_mode {
            let style = ProgressStyle::with_template(
                "{spinner} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})\n{msg}",
            )?
            .progress_chars("#>-")
            .tick_chars("-\\|/");
            pb.set_style(style);
        }

        for volume in volumes {
            checkpoint.add_volume(volume.folder_name.clone())?;
            let volume_dir = usb_mount.join(&volume.folder_name);

            fs::create_dir_all(&volume_dir)
                .with_context(|| format!("Failed to create volume {}", volume_dir.display()))?;

            for file in volume.files {
                let dest = volume_dir.join(&file.sanitized_name);
                let original_hash = backup
                    .as_ref()
                    .and_then(|b| b.checksums.get(&file.source_path))
                    .cloned()
                    .unwrap_or_default();

                checkpoint.record_file_start(
                    global_idx,
                    file.source_path.clone(),
                    file.sanitized_name.clone(),
                    original_hash,
                )?;
                if sync_mode {
                    mirror_checkpoint_to_usb(&checkpoint, usb_mount)?;
                }
                pb.set_message(format!(
                    "Normalizing: {} -> {}",
                    volume.folder_name, file.sanitized_name
                ));

                // INTEGRACION DEL NORMALIZER A TRAVES DE FFMPEG
                match normalizer::normalize_audio(&file.source_path, &dest) {
                    Ok(_) => {
                        let usb_checksum = compute_sha256(&dest)?;
                        checkpoint.mark_file_completed(global_idx, usb_checksum)?;
                        if sync_mode {
                            mirror_checkpoint_to_usb(&checkpoint, usb_mount)?;
                        }
                        pb.inc(1);
                        let files_processed = global_idx + 1;
                        let percentage = if audio_files.is_empty() {
                            100.0
                        } else {
                            (files_processed as f64 / audio_files.len() as f64) * 100.0
                        };
                        let elapsed = start.elapsed().as_secs();
                        let eta_seconds = if files_processed == 0 {
                            0
                        } else {
                            let avg = elapsed / files_processed as u64;
                            avg.saturating_mul(audio_files.len() as u64 - files_processed as u64)
                        };
                        IpcEvent::Progress {
                            files_processed,
                            total_files: audio_files.len(),
                            percentage,
                            current_file: format!("{}/{}", volume.folder_name, file.sanitized_name),
                            eta_seconds,
                        }
                        .emit(json_mode);
                    }
                    Err(e) => {
                        if matches!(
                            e.downcast_ref::<ProvisioningError>(),
                            Some(ProvisioningError::DrmProtected { .. })
                        ) {
                            checkpoint.mark_file_failed(global_idx, "Skipped_DRM".to_string())?;
                            if sync_mode {
                                mirror_checkpoint_to_usb(&checkpoint, usb_mount)?;
                            }
                            skipped_drm += 1;
                            if let Some(backup_meta) = backup.as_ref() {
                                append_drm_skip_log(&backup_meta.backup_dir, &file.source_path);
                            }
                            if !json_mode {
                                pb.println(format!(
                                    "[SKIP DRM] {}",
                                    file.source_path.to_string_lossy()
                                ));
                            }
                            IpcEvent::Warning {
                                code: "DRM_SKIPPED".to_string(),
                                source_file: file.source_path.to_string_lossy().to_string(),
                                message:
                                    "El archivo esta protegido por cifrado DRM y fue ignorado."
                                        .to_string(),
                            }
                            .emit(json_mode);

                            let files_processed = global_idx + 1;
                            let percentage = if audio_files.is_empty() {
                                100.0
                            } else {
                                (files_processed as f64 / audio_files.len() as f64) * 100.0
                            };
                            let elapsed = start.elapsed().as_secs();
                            let eta_seconds = if files_processed == 0 {
                                0
                            } else {
                                let avg = elapsed / files_processed as u64;
                                avg.saturating_mul(
                                    audio_files.len() as u64 - files_processed as u64,
                                )
                            };
                            IpcEvent::Progress {
                                files_processed,
                                total_files: audio_files.len(),
                                percentage,
                                current_file: format!(
                                    "{}/{}",
                                    volume.folder_name, file.sanitized_name
                                ),
                                eta_seconds,
                            }
                            .emit(json_mode);

                            pb.inc(1);
                            global_idx += 1;
                            continue;
                        }

                        checkpoint.mark_file_failed(global_idx, e.to_string())?;
                        pb.finish_with_message("Operation aborted due to normalization error.");
                        return Err(anyhow::anyhow!(
                            "Normalization Error on {}: {}",
                            dest.display(),
                            e
                        ));
                    }
                }
                global_idx += 1;
            }

            if let Ok(dir_file) = fs::File::open(&volume_dir) {
                let _ = dir_file.sync_all();
            }
        }
        pb.finish_with_message("Physical distribution and normalization completed.");
        human_out(json_mode, "Physical distribution complete.");

        // Paso 6: Verificacion real de invariantes de hardware + integridad criptografica.
        human_out(json_mode, "\nStep 6: Hardware Invariant Verification...");
        std::env::set_var("LAP_JSON_MODE", if json_mode { "1" } else { "0" });
        let checkpoint_data = checkpoint.get_data();
        let report = verification::pre_eject_verification(usb_mount, checkpoint_data)?;

        if !report.success {
            return Err(anyhow::anyhow!(
                "Provisioning failed final QA. Check logs for details."
            ));
        }

        checkpoint.finalize()?;
        if sync_mode {
            mirror_checkpoint_to_usb(&checkpoint, usb_mount)?;
        }
        human_out(json_mode, "Checkpoint finalized after QA.");

        if !dry_run {
            human_out(json_mode, "\nStep 7: Safe Ejection...");
            verification::safe_eject(&device.device_path, usb_mount)?;
        }

        IpcEvent::Success {
            total_processed: audio_files.len().saturating_sub(skipped_drm),
            total_skipped: skipped_drm,
            elapsed_time_seconds: start.elapsed().as_secs(),
            message: format!(
                "Provision completada y dispositivo desmontado de forma segura. {} archivo(s) aislado(s) en .legacy_quarantine.",
                quarantined_count
            ),
        }
        .emit(json_mode);
    }

    human_out(json_mode, "\n=== Provisioning Complete ===");
    Ok(())
}

fn compute_sha256(file_path: &std::path::Path) -> Result<String> {
    let mut file = std::fs::File::open(file_path)
        .with_context(|| format!("Failed to open file for hashing: {}", file_path.display()))?;
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

fn mirror_checkpoint_to_usb(
    checkpoint_mgr: &checkpoint::CheckpointManager,
    usb_mount: &Path,
) -> Result<()> {
    let checkpoint_path = usb_mount.join(".provisioning_checkpoint");
    let tmp_path = usb_mount.join(".provisioning_checkpoint.tmp");
    let serialized = serde_json::to_string_pretty(checkpoint_mgr.get_data())?;

    let mut tmp_file = fs::File::create(&tmp_path)?;
    tmp_file.write_all(serialized.as_bytes())?;
    tmp_file.sync_all()?;

    fs::rename(&tmp_path, &checkpoint_path)?;

    if let Ok(root_dir) = fs::File::open(usb_mount) {
        let _ = root_dir.sync_all();
    }

    Ok(())
}

fn resume_provisioning(
    backup_dir: &std::path::Path,
    usb_mount: &std::path::Path,
    json_mode: bool,
) -> Result<()> {
    human_out(json_mode, "\n=== Resuming USB Provisioning ===");
    human_out(
        json_mode,
        &format!("Backup Directory: {}", backup_dir.display()),
    );
    human_out(json_mode, &format!("USB Target: {}", usb_mount.display()));

    human_out(json_mode, "Step 1: Validating USB device...");
    let device = hardware::validate_device_path(usb_mount)?;
    device.is_valid_for_provisioning()?;

    // R-18: INYECCIÓN del Lock de exclusión mutua antes de cualquier I/O en la USB.
    let _lock = hardware::ProvisioningLock::acquire(usb_mount)?;

    // R-20: Validacion de I/O fisica (Dirty Bit Test)
    hardware::assert_rw_filesystem(usb_mount)?;

    human_out(
        json_mode,
        &format!(
            "USB device validated (RW & Locked): {}",
            usb_mount.display()
        ),
    );

    let checkpoint_file = backup_dir.join(".provisioning_checkpoint");
    if !checkpoint_file.exists() {
        return Err(anyhow::anyhow!("❌ No se encontro archivo de checkpoint."));
    }

    let mut checkpoint_mgr = checkpoint::CheckpointManager::load_from_disk(&checkpoint_file)?;

    if !checkpoint_mgr.get_data().is_recoverable() {
        human_out(
            json_mode,
            "La sesion registrada ya esta completada o no es recuperable.",
        );
        return Ok(());
    }

    human_out(
        json_mode,
        &format!(
            "Progreso anterior: {:.1}%",
            checkpoint_mgr.get_data().progress_percentage()
        ),
    );

    let recovery_mgr =
        recovery::RecoveryManager::new(backup_dir.to_path_buf(), usb_mount.to_path_buf());
    recovery_mgr.execute_recovery(&mut checkpoint_mgr)?;

    IpcEvent::Success {
        total_processed: checkpoint_mgr
            .get_data()
            .processed_files
            .values()
            .filter(|f| f.status == checkpoint::OperationStatus::Completed)
            .count(),
        total_skipped: checkpoint_mgr
            .get_data()
            .processed_files
            .values()
            .filter(|f| f.error_message.as_deref() == Some("Skipped_DRM"))
            .count(),
        elapsed_time_seconds: 0,
        message: "Recovery completada correctamente.".to_string(),
    }
    .emit(json_mode);

    human_out(json_mode, "\n=== Recovery Complete ===");
    Ok(())
}
