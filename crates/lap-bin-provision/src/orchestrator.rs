use anyhow::{Context, Result};
use lap_core::crypto::compute_file_sha256;
use lap_core::error::ProvisioningError;
use lap_core::ipc::IpcEvent;
use lap_core::security::validate_path_containment;
use lap_core::{
    audio_discovery, backup, checkpoint, diffing, distribution, hardware, ingestion, journal,
    in_place_transformer::InPlaceTransformer, manifest, normalizer, recovery, sanitizer, verification,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::reporter::ProgressReporter;

/// [R-01-006] EntryPoint Delgada
/// Precondicion: el entrypoint CLI ya resolvio argumentos y contexto de ejecucion.
/// Postcondicion: el flujo de negocio completo de provisionamiento se ejecuta desde este orquestador.
/// Invariante: comandos y subflujos reutilizan una unica capa de orquestacion desacoplada de `main`.
pub struct ProvisioningOrchestrator {
    reporter: Box<dyn ProgressReporter>,
    json_mode: bool,
}

impl ProvisioningOrchestrator {
    pub fn new(reporter: Box<dyn ProgressReporter>, json_mode: bool) -> Self {
        Self {
            reporter,
            json_mode,
        }
    }

    pub fn list_usb_devices(&mut self) -> Result<()> {
        self.reporter.info("\n=== Detecting USB Devices ===\n");
        let devices = hardware::detect_usb_devices()?;
        if devices.is_empty() {
            self.reporter.info("No USB/removable devices detected.");
            return Ok(());
        }
        self.reporter
            .info(&format!("Found {} USB device(s):\n", devices.len()));
        for (idx, device) in devices.iter().enumerate() {
            self.reporter
                .info(&format!("  [{}] Device: {}", idx + 1, device.device_path.display()));
            self.reporter
                .info(&format!("      Mount point: {}", device.mount_point.display()));
            self.reporter
                .info(&format!("      Filesystem: {}", device.fs_type));
            if let Some(allocation_unit_bytes) = device.allocation_unit_bytes {
                self.reporter.info(&format!(
                    "      Allocation unit: {} KB{}",
                    allocation_unit_bytes / 1024,
                    if allocation_unit_bytes
                        == hardware::LEGACY_FAT32_ALLOCATION_UNIT_BYTES
                    {
                        " (legacy cache OK)"
                    } else {
                        " (reformat recommended)"
                    }
                ));
            } else {
                self.reporter
                    .info("      Allocation unit: unknown (best-effort verification)");
            }
            self.reporter
                .info(&format!("      Size: {:.2} GB", device.size_gb()));
            self.reporter.info(&format!(
                "      Removable: {}",
                if device.is_removable { "Yes" } else { "No" }
            ));
            self.reporter.info("");
        }
        Ok(())
    }

    pub fn scan_usb_audio(&mut self, usb: Option<&Path>) -> Result<()> {
        let mountpoint = if let Some(path) = usb {
            path.to_path_buf()
        } else {
            let devices = hardware::detect_usb_devices()?;
            if devices.is_empty() {
                self.reporter.info("No USB devices detected.");
                return Ok(());
            }
            devices[0].mount_point.clone()
        };

        self.reporter.info(&format!(
            "Scanning for audio files on {}...",
            mountpoint.display()
        ));

        let report = audio_discovery::discover_audio_files(&mountpoint)?;

        if report.audio_files.is_empty() {
            self.reporter.info("No audio files found.");
            return Ok(());
        }

        self.reporter
            .info(&format!("Found {} audio file(s)", report.audio_files.len()));
        self.reporter
            .info(&format!("Total size: {:.2} MB", report.total_size_mb()));
        Ok(())
    }

    pub fn format_usb_for_legacy(
        &mut self,
        usb_mount: &Path,
        confirm_device: &str,
        label: Option<&str>,
        force_reformat: bool,
    ) -> Result<()> {
        self.reporter.info("\n=== Starting Legacy USB Reformat ===");

        let device = hardware::inspect_device_path(usb_mount)?;
        if !device.is_removable {
            return Err(anyhow::anyhow!(ProvisioningError::InvalidConfig {
                details: format!(
                    "El destino '{}' no es removible; formateo abortado.",
                    device.device_path.display()
                ),
            }));
        }

        if confirm_device.trim() != device.device_path.to_string_lossy() {
            return Err(anyhow::anyhow!(ProvisioningError::InvalidConfig {
                details: format!(
                    "Confirmacion invalida. Use --confirm-device '{}' para autorizar el borrado.",
                    device.device_path.display()
                ),
            }));
        }

        let needs_reformat = device.requires_legacy_reformat();
        if !needs_reformat && !force_reformat {
            self.reporter.info(&format!(
                "La USB '{}' ya cumple el perfil legacy; se omite el formateo.",
                usb_mount.display()
            ));
            IpcEvent::Success {
                total_processed: 0,
                total_skipped: 0,
                elapsed_time_seconds: 0,
                message: "Formateo omitido: el volumen ya cumple el perfil legacy.".to_string(),
            }
            .emit(self.json_mode);
            return Ok(());
        }

        self.reporter.info("Step 1: Creating pre-format backup...");
        let backup_home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let backup_home_path = Path::new(&backup_home);
        let mut backup_meta = backup::BackupMetadata::new_for_target(
            backup_home_path,
            &format!("preformat__{}", device.backup_identity_key()),
        )?;

        let backed_files = Self::backup_usb_tree(&mut backup_meta, usb_mount, usb_mount)?;

        if !backup_meta.verify_backup()? {
            return Err(anyhow::anyhow!(
                "Backup verification failed before reformat"
            ));
        }

        self.reporter.info(&format!(
            "Pre-format backup verified. Directory: {} ({} file(s))",
            backup_meta.backup_dir.display(),
            backed_files
        ));

        self.reporter.info("Step 2: Formatting USB to FAT32 with 32 KB allocation unit...");
        hardware::format_device_for_legacy(&device, usb_mount, label)?;

        let reformatted_device = hardware::validate_device_path(usb_mount)?;
        self.reporter.info(&format!(
            "USB reformatted and remounted: {} [{} KB cluster]",
            reformatted_device.mount_point.display(),
            reformatted_device
                .allocation_unit_bytes
                .unwrap_or(hardware::LEGACY_FAT32_ALLOCATION_UNIT_BYTES)
                / 1024
        ));

        IpcEvent::Success {
            total_processed: backed_files,
            total_skipped: 0,
            elapsed_time_seconds: 0,
            message: format!(
                "USB reformateada correctamente a FAT32 legacy. Backup en '{}'.",
                backup_meta.backup_dir.display()
            ),
        }
        .emit(self.json_mode);

        Ok(())
    }

    pub fn validate_canonical_paths(
        &self,
        usb_mount: &Path,
        audio_source: &Path,
    ) -> std::result::Result<(), ProvisioningError> {
        let usb_can = usb_mount
            .canonicalize()
            .map_err(|e| ProvisioningError::InvalidConfig {
                details: format!(
                    "No se pudo resolver la ruta USB '{}': {}",
                    usb_mount.display(),
                    e
                ),
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

    pub fn ingest_staging(&mut self, usb: &Path, source: &Path) -> Result<()> {
        self.reporter.info("\n=== Starting Audio Ingestion ===");
        self.reporter.info(&format!(
            "USB: {} | Staging: {}",
            usb.display(),
            source.display()
        ));

        let manifest = ingestion::ingest_audio_files(usb, source, self.json_mode)?;

        self.reporter.info("\nIngestion complete.");
        self.reporter
            .info(&format!("Staging directory: {}", manifest.staging_dir.display()));
        self.reporter
            .info(&format!("Audio files copied: {}", manifest.files.len()));
        self.reporter.info(&format!(
            "Total size: {:.2} MB",
            manifest.total_bytes as f64 / 1_048_576.0
        ));

        IpcEvent::Success {
            total_processed: manifest.files.len(),
            total_skipped: 0,
            elapsed_time_seconds: 0,
            message: format!(
                "Ingesta completada: {} archivos en '{}'",
                manifest.files.len(),
                manifest.staging_dir.display()
            ),
        }
        .emit(self.json_mode);

        Ok(())
    }

    pub fn refactor_usb(&mut self, usb: &Path, source: &Path, keep_staging: bool) -> Result<()> {
        self.reporter.info("\n=== Starting In-Situ Refactor ===");
        self.reporter.info(&format!(
            "USB: {} | Work dir: {}",
            usb.display(),
            source.display()
        ));

        let usb_can = usb
            .canonicalize()
            .map_err(|e| ProvisioningError::InvalidConfig {
                details: format!("No se pudo resolver la ruta USB '{}': {}", usb.display(), e),
            })?;
        let source_abs = if source.is_absolute() {
            source.to_path_buf()
        } else {
            std::env::current_dir()?.join(source)
        };
        if source_abs.starts_with(&usb_can) {
            return Err(anyhow::anyhow!(ProvisioningError::InvalidConfig {
                details: format!(
                    "El staging '{}' no puede estar dentro de la USB de destino '{}'.",
                    source_abs.display(),
                    usb_can.display()
                ),
            }));
        }

        self.ingest_staging(usb, source)?;
        self.validate_canonical_paths(usb, source)?;
        self.provision_usb(usb, source, false, true, false)?;

        if !keep_staging {
            fs::remove_dir_all(source)
                .with_context(|| format!("No se pudo eliminar el staging '{}'", source.display()))?;
            self.reporter.info("Staging local eliminado.");
        }

        Ok(())
    }

    fn provision_usb_in_place_rebuild(&mut self, usb_mount: &Path, dry_run: bool) -> Result<()> {
        let start = Instant::now();
        self.reporter
            .info("\n=== Starting In-Place USB Rebuild (rename-only) ===");
        if dry_run {
            self.reporter.info("[DRY RUN] No actual changes will be made");
        }

        self.reporter.info("Step 1: Validating USB device...");
        let device = hardware::validate_device_path(usb_mount)?;
        device.is_valid_for_provisioning()?;
        hardware::assert_hardware_health(&device.device_path)?;

        let _lock = hardware::ProvisioningLock::acquire(usb_mount)?;
        hardware::assert_rw_filesystem(usb_mount)?;

        self.reporter
            .info("Step 2: Building in-place plan (smart pass-through sin staging)...");
        let plan = InPlaceTransformer::build_plan(usb_mount)?;

        if plan.entries.is_empty() {
            self.reporter.info("No audio files found to rebuild.");
            IpcEvent::Success {
                total_processed: 0,
                total_skipped: 0,
                elapsed_time_seconds: start.elapsed().as_secs(),
                message: "Rebuild in-place completado: no se encontraron archivos de audio."
                    .to_string(),
            }
            .emit(self.json_mode);
            return Ok(());
        }

        self.reporter.info(&format!(
            "Plan in-place: {} archivo(s) en {} volumen(es)",
            plan.entries.len(),
            plan.entries.len().div_ceil(distribution::MAX_FILES_PER_FOLDER)
        ));

        if dry_run {
            IpcEvent::Success {
                total_processed: 0,
                total_skipped: 0,
                elapsed_time_seconds: start.elapsed().as_secs(),
                message: format!(
                    "Dry-run in-place: {} renombre(s) potencial(es) planificado(s).",
                    plan.entries
                        .iter()
                        .filter(|e| e.source_path != e.destination_path)
                        .count()
                ),
            }
            .emit(self.json_mode);
            return Ok(());
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let checkpoint_dir = Path::new(&home)
            .join(".legacy_audio_provisioner")
            .join("checkpoints")
            .join(format!("in_place_{}", device.backup_identity_key()));
        fs::create_dir_all(&checkpoint_dir).with_context(|| {
            format!(
                "No se pudo crear directorio de checkpoint '{}'",
                checkpoint_dir.display()
            )
        })?;

        let checkpoint_file = checkpoint_dir.join(".provisioning_checkpoint");
        let mut resumed_mode = false;
        let mut checkpoint = if checkpoint_file.exists() {
            match checkpoint::CheckpointManager::load_from_disk(&checkpoint_file) {
                Ok(existing) => {
                    let data = existing.get_data();
                    if data.operation_status == checkpoint::OperationStatus::InProgress
                        && data.total_files == plan.entries.len()
                    {
                        resumed_mode = true;
                        self.reporter.info(&format!(
                            "Checkpoint in-place detectado (sesion {}). Reanudando...",
                            data.session_id
                        ));
                        existing
                    } else {
                        self.reporter.info(
                            "Checkpoint previo no reanudable (completado o total distinto). Se iniciara una nueva sesion in-place.",
                        );
                        checkpoint::CheckpointManager::new(
                            checkpoint_dir,
                            usb_mount.to_path_buf(),
                            usb_mount.to_path_buf(),
                            plan.entries.len(),
                        )?
                    }
                }
                Err(_) => checkpoint::CheckpointManager::new(
                    checkpoint_dir,
                    usb_mount.to_path_buf(),
                    usb_mount.to_path_buf(),
                    plan.entries.len(),
                )?,
            }
        } else {
            checkpoint::CheckpointManager::new(
                checkpoint_dir,
                usb_mount.to_path_buf(),
                usb_mount.to_path_buf(),
                plan.entries.len(),
            )?
        };
        checkpoint.set_auto_persist(false);

        self.reporter
            .info("Step 3: Applying smart pass-through (rename limpio / ffmpeg sucio)...");
        self.reporter.start_progress(plan.entries.len() as u64)?;

        let temp_root = validate_path_containment(usb_mount, Path::new(".in_place_rebuild_tmp"))?;
        if resumed_mode && temp_root.exists() {
            // Si hubo corte previo, descartamos artefactos intermedios para retomar desde estado estable.
            let _ = fs::remove_dir_all(&temp_root);
        }
        fs::create_dir_all(&temp_root)?;

        let resumed_completed_hashes: HashMap<usize, String> = checkpoint
            .get_data()
            .processed_files
            .iter()
            .filter_map(|(idx, cp)| {
                if cp.status == checkpoint::OperationStatus::Completed {
                    cp.usb_checksum.clone().map(|h| (*idx, h))
                } else {
                    None
                }
            })
            .collect();

        let mut resumed_skipped = 0usize;
        let mut staged_entries: Vec<(usize, PathBuf, PathBuf, String, String)> = Vec::new();

        for entry in &plan.entries {
            if let Some(expected_hash) = resumed_completed_hashes.get(&entry.index) {
                let already_completed = entry.destination_path.exists()
                    && compute_file_sha256(&entry.destination_path)
                        .map(|actual| actual == *expected_hash)
                        .unwrap_or(false);
                if already_completed {
                    resumed_skipped += 1;
                    self.reporter.inc_progress(
                        1,
                        &format!("[RESUME] already completed: {}", entry.normalized_name),
                    );
                    continue;
                }
            }

            checkpoint.record_file_start(
                entry.index,
                entry.source_path.clone(),
                entry.normalized_name.clone(),
                String::new(),
            )?;

            let temp_name = format!("{:06}_{}", entry.index + 1, entry.normalized_name);
            let temp_path = validate_path_containment(&temp_root, Path::new(&temp_name))?;

            fs::rename(&entry.source_path, &temp_path).with_context(|| {
                format!(
                    "No se pudo mover temporalmente '{}' a '{}'",
                    entry.source_path.display(),
                    temp_path.display()
                )
            })?;

            staged_entries.push((
                entry.index,
                temp_path,
                entry.destination_path.clone(),
                entry.volume_name.clone(),
                entry.normalized_name.clone(),
            ));
        }

        if resumed_skipped > 0 {
            self.reporter.info(&format!(
                "Reanudacion in-place: {} archivo(s) ya completado(s) fueron omitidos.",
                resumed_skipped
            ));
        }

        let mut current_volume = String::new();
        for (position, (index, temp_path, destination_path, volume_name, normalized_name)) in
            staged_entries.into_iter().enumerate()
        {
            if current_volume != volume_name {
                current_volume = volume_name.clone();
                checkpoint.add_volume(volume_name.clone())?;
            }

            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let processing = normalizer::classify_audio_processing(&temp_path)?;

            match processing {
                normalizer::ProcessingDecision::FastInPlaceRename => {
                    fs::rename(&temp_path, &destination_path).with_context(|| {
                        format!(
                            "No se pudo mover '{}' a '{}'",
                            temp_path.display(),
                            destination_path.display()
                        )
                    })?;
                }
                normalizer::ProcessingDecision::FfmpegCopyClean
                | normalizer::ProcessingDecision::FfmpegTranscode => {
                    normalizer::normalize_audio(&temp_path, &destination_path, processing)?;
                    let _ = fs::remove_file(&temp_path);
                }
            }

            let usb_checksum = compute_file_sha256(&destination_path)?;
            checkpoint.mark_file_completed(index, usb_checksum)?;

            let files_processed = position + 1;
            self.reporter
                .inc_progress(1, &format!("Processing: {}/{}", volume_name, normalized_name));

            if files_processed.is_multiple_of(distribution::MAX_FILES_PER_FOLDER)
                || files_processed == plan.entries.len()
            {
                checkpoint.save_to_disk()?;
            }
        }

        if temp_root.exists() {
            let _ = fs::remove_dir_all(&temp_root);
        }

        self.reporter.info("Step 4: Final verification...");
        let report = verification::pre_eject_verification(usb_mount, checkpoint.get_data(), self.json_mode)?;
        if !report.success {
            return Err(anyhow::anyhow!(
                "In-place rebuild failed final QA. Check logs for details."
            ));
        }

        checkpoint.finalize()?;
        Self::mirror_checkpoint_to_usb(&checkpoint, usb_mount)?;

        self.reporter.info("Step 5: Safe ejection...");
        verification::safe_eject(&device.device_path, usb_mount)?;

        self.reporter
            .finish("In-place metadata rebuild completed successfully.");

        IpcEvent::Success {
            total_processed: plan.entries.len(),
            total_skipped: 0,
            elapsed_time_seconds: start.elapsed().as_secs(),
            message: "Rebuild in-place completado con smart pass-through (rename/transcode condicional) y sin staging.".to_string(),
        }
        .emit(self.json_mode);

        Ok(())
    }

    pub fn provision_usb(
        &mut self,
        usb_mount: &Path,
        audio_source: &Path,
        dry_run: bool,
        sync_mode: bool,
        in_place_rebuild: bool,
    ) -> Result<()> {
        if in_place_rebuild {
            return self.provision_usb_in_place_rebuild(usb_mount, dry_run);
        }

        let start = Instant::now();
        self.reporter.info("\n=== Starting USB Provisioning ===");
        if dry_run {
            self.reporter.info("[DRY RUN] No actual changes will be made");
        }

        self.reporter.info("Step 1: Validating USB device...");
        let device = hardware::validate_device_path(usb_mount)?;
        device.is_valid_for_provisioning()?;

        // [R-02-009] Sonda de salud a nivel de controlador
        hardware::assert_hardware_health(&device.device_path)?;

        let _lock = hardware::ProvisioningLock::acquire(usb_mount)?;
        hardware::assert_rw_filesystem(usb_mount)?;

        self.reporter.info(&format!(
            "USB device validated (RW & Locked): {}",
            usb_mount.display()
        ));

        self.reporter
            .info("\nStep 2: Scanning audio files (Secure Mode)...");
        let discovery_report = audio_discovery::discover_audio_files(audio_source)?;
        let discovered_source_files = discovery_report.audio_files;
        self.reporter.info(&format!(
            "Found {} audio files",
            discovered_source_files.len()
        ));

        if discovered_source_files.is_empty() {
            return Err(anyhow::anyhow!(
                "No valid audio files found in source. Aborting."
            ));
        }

        let mut next_global_index = 1usize;
        let mut existing_volume_counts = std::collections::BTreeMap::new();
        let mut checkpoint_known_names: HashSet<String> = HashSet::new();
        let mut displaced_in_target: std::collections::HashMap<PathBuf, PathBuf> =
            std::collections::HashMap::new();
        let mut move_journal: Option<journal::JournalManager> = None;

        if sync_mode {
            self.reporter
                .info("Step 2.1: Incremental sync mode enabled (USB hash diff)...");

            let usb_checkpoint_path = usb_mount.join(".provisioning_checkpoint");
            if usb_checkpoint_path.exists() {
                if let Ok(usb_checkpoint_mgr) = checkpoint::CheckpointManager::load_from_disk(&usb_checkpoint_path) {
                    for file_cp in usb_checkpoint_mgr.get_data().processed_files.values() {
                        checkpoint_known_names.insert(file_cp.normalized_name.clone());
                        if let Some((prefix, _)) = file_cp.normalized_name.split_once('_') {
                            if let Ok(parsed) = prefix.parse::<usize>() {
                                next_global_index = next_global_index.max(parsed.saturating_add(1));
                            }
                        }
                    }
                    self.reporter.info(&format!(
                        "Checkpoint USB cargado: {} entradas conocidas",
                        checkpoint_known_names.len()
                    ));
                }
            }
        }

        let (audio_files, skipped_existing, mut untracked_in_target) = if sync_mode {
            let diff_report = diffing::calculate_sync_diff(
                &discovered_source_files,
                usb_mount,
                &checkpoint_known_names,
            )?;

            next_global_index = next_global_index.max(diff_report.max_existing_index.saturating_add(1));
            existing_volume_counts = diff_report.existing_volume_counts;
            displaced_in_target = diff_report.displaced_in_target;

            (
                diff_report.files_to_process,
                diff_report.skipped_existing,
                diff_report.untracked_in_target,
            )
        } else {
            (discovered_source_files, 0usize, Vec::new())
        };

        if sync_mode {
            self.reporter.info(&format!(
                "Diff completo: {} a reprocesar, {} ya existentes (skip), {} untracked en USB, {} displaced para move in-place",
                audio_files.len(),
                skipped_existing,
                untracked_in_target.len(),
                displaced_in_target.len()
            ));

            let mut mgr = journal::JournalManager::load_or_create(usb_mount)?;
            let summary = mgr.reconcile(usb_mount)?;
            if summary.total > 0 {
                self.reporter.info(&format!(
                    "Journal detectado: {} transacciones ({} committed, {} pending, {} failed)",
                    summary.total, summary.committed, summary.pending, summary.failed
                ));
            }
            move_journal = Some(mgr);
        }

        let root_topology = diffing::analyze_root_topology(usb_mount)?;
        let root_sandbox_candidates = if matches!(
            root_topology.policy,
            diffing::RootContentPolicy::ManagedTopology
        ) {
            root_topology.non_whitelisted_entries.clone()
        } else {
            Vec::new()
        };

        match root_topology.policy {
            diffing::RootContentPolicy::ManagedTopology => {
                if !root_sandbox_candidates.is_empty() {
                    self.reporter.info(&format!(
                        "Topology sandbox detecto {} entrada(s) raiz fuera de whitelist.",
                        root_sandbox_candidates.len()
                    ));
                }
            }
            diffing::RootContentPolicy::PreserveUserContent => {
                self.reporter.info(&format!(
                    "USB no gestionada detectada: se preservaran {} entrada(s) raiz del usuario fuera de VOL_XX.",
                    root_topology.non_whitelisted_entries.len()
                ));
            }
            diffing::RootContentPolicy::Empty => {}
        }

        if matches!(
            root_topology.policy,
            diffing::RootContentPolicy::PreserveUserContent
        ) && !untracked_in_target.is_empty()
        {
            self.reporter.info(&format!(
                "USB no gestionada: {} archivo(s) existentes fuera de topologia legacy se conservaran sin mover a cuarentena.",
                untracked_in_target.len()
            ));
            untracked_in_target.clear();
        }

        if sync_mode
            && audio_files.is_empty()
            && untracked_in_target.is_empty()
            && root_sandbox_candidates.is_empty()
        {
            let removed_dirs = Self::prune_empty_non_compliant_root_dirs(usb_mount)?;
            if removed_dirs > 0 {
                self.reporter.info(&format!(
                    "Limpieza topologica: {} carpeta(s) raiz no estandar vacia(s) eliminada(s).",
                    removed_dirs
                ));
            }

            self.reporter
                .info("No hay cambios: la USB ya esta sincronizada y no existen archivos huerfanos.");
            IpcEvent::Success {
                total_processed: 0,
                total_skipped: skipped_existing,
                elapsed_time_seconds: start.elapsed().as_secs(),
                message: "Sincronizacion completada: no habia archivos nuevos ni huérfanos por aislar."
                    .to_string(),
            }
            .emit(self.json_mode);
            return Ok(());
        }

        self.reporter.info("\nStep 3: Validating backup capacity...");
        let backup_home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let backup_home_path = Path::new(&backup_home);
        let new_audio_size: u64 = audio_files.iter().map(|f| f.size_bytes).sum();
        let untracked_backup_size: u64 = untracked_in_target
            .iter()
            .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
            .sum();
        let root_sandbox_backup_size: u64 = root_sandbox_candidates
            .iter()
            .map(|p| Self::total_path_bytes(p))
            .sum();
        let total_size = new_audio_size
            .saturating_add(untracked_backup_size)
            .saturating_add(root_sandbox_backup_size);

        backup::check_disk_space(total_size, backup_home_path)?;

        let stable_backup_key = device.backup_identity_key();

        let mut backup = if dry_run {
            None
        } else {
            Some(backup::BackupMetadata::new_for_target(
                backup_home_path,
                &stable_backup_key,
            )?)
        };

        let mut quarantined_count = 0usize;
        let mut topology_quarantined_count = 0usize;

        if let Some(backup_meta) = backup.as_mut() {
            if !root_sandbox_candidates.is_empty() {
                self.reporter
                    .info("Step 3.0: Topology sandbox (quarantine universal de raiz, backup-first)...");

                let session_label = format!("topology_{}", stable_backup_key);
                let topology_report = diffing::quarantine_non_whitelisted_root_entries(
                    usb_mount,
                    &root_sandbox_candidates,
                    backup_meta,
                    &session_label,
                )?;

                topology_quarantined_count = topology_report.quarantined.len();
                if !topology_report.failed.is_empty() {
                    for (path, details) in topology_report.failed.iter().take(5) {
                        IpcEvent::Warning {
                            code: "TOPOLOGY_QUARANTINE_FAILED".to_string(),
                            source_file: path.display().to_string(),
                            message: details.clone(),
                        }
                        .emit(self.json_mode);
                    }
                }

                self.reporter.info(&format!(
                    "Topology sandbox: {} entrada(s) movida(s), {} fallida(s)",
                    topology_report.quarantined.len(),
                    topology_report.failed.len()
                ));

                untracked_in_target.retain(|p| p.exists());
            }

            for file in &audio_files {
                if let Some(usb_path) = displaced_in_target.get(&file.path) {
                    if usb_path.exists() {
                        backup_meta.backup_file(usb_path)?;
                        continue;
                    }
                }

                backup_meta.backup_file(&file.path)?;
            }

            if sync_mode && !untracked_in_target.is_empty() {
                self.reporter
                    .info("Step 3.1: Aislando huérfanos en .legacy_quarantine (backup-first)...");
                let session_label = format!("sync_{}", stable_backup_key);
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
                        .emit(self.json_mode);
                    }
                }

                self.reporter.info(&format!(
                    "Cuarentena completada: {} movidos, {} fallidos",
                    quarantine_report.quarantined.len(),
                    quarantine_report.failed.len()
                ));
            }

            if !backup_meta.verify_backup()? {
                return Err(anyhow::anyhow!(
                    "Backup verification failed - corrupted files detected"
                ));
            }
            self.reporter.info(&format!(
                "Backup verified. Directory: {}",
                backup_meta.backup_dir.display()
            ));
        }

        if audio_files.is_empty() {
            self.reporter
                .info("No hay archivos nuevos para procesar en modo --sync.");
            IpcEvent::Success {
                total_processed: 0,
                total_skipped: skipped_existing,
                elapsed_time_seconds: start.elapsed().as_secs(),
                message: format!(
                    "Sincronizacion completada: sin cambios nuevos; {} archivo(s) aislado(s) en .legacy_quarantine (topologia:{} + huérfanos:{}).",
                    topology_quarantined_count + quarantined_count,
                    topology_quarantined_count,
                    quarantined_count
                ),
            }
            .emit(self.json_mode);
            return Ok(());
        }

        self.reporter
            .info("\nStep 4: Forcing MP3 extension, Sanitizing & Initializing Checkpoint...");
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
        checkpoint.set_auto_persist(false);

        let mut file_mappings: Vec<(PathBuf, String)> = Vec::with_capacity(audio_files.len());
        for (idx, file) in audio_files.iter().enumerate() {
            let stem = file
                .path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("audio");

            let content_hash = backup
                .as_ref()
                .and_then(|b| b.checksums.get(&file.path))
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| {
                    compute_file_sha256(&file.path).with_context(|| {
                        format!(
                            "No se pudo calcular SHA256 para naming compacto de '{}'",
                            file.path.display()
                        )
                    })
                })?;

            let compact_name = sanitizer::build_hashed_legacy_name(
                stem,
                idx + next_global_index,
                &content_hash,
            );
            file_mappings.push((file.path.clone(), compact_name));
        }

        let volumes = if sync_mode {
            diffing::plan_incremental_distribution(file_mappings, &existing_volume_counts)
        } else {
            distribution::plan_distribution(file_mappings)?
        };
        self.reporter
            .info(&format!("Planned {} volume(s)", volumes.len()));

        if sync_mode {
            let mut move_candidates = 0usize;
            for volume in &volumes {
                for file in &volume.files {
                    let is_mp3_source = file
                        .source_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("mp3"))
                        .unwrap_or(false);

                    if !is_mp3_source {
                        continue;
                    }

                    if let Some(displaced_usb_path) = displaced_in_target.get(&file.source_path) {
                        let decision = normalizer::classify_audio_processing(&file.source_path)?;
                        if decision != normalizer::ProcessingDecision::FastInPlaceRename {
                            continue;
                        }

                        let volume_dir = validate_path_containment(
                            usb_mount,
                            Path::new(&volume.folder_name),
                        )
                        .with_context(|| {
                            format!(
                                "R-05: Violacion de contencion en volumen: {}",
                                volume.folder_name
                            )
                        })?;
                        let target_abs = validate_path_containment(
                            &volume_dir,
                            Path::new(&file.sanitized_name),
                        )
                        .with_context(|| {
                            format!(
                                "R-05: Violacion de contencion en destino: {}",
                                file.sanitized_name
                            )
                        })?;
                        let source_rel = Self::to_usb_relative(displaced_usb_path, usb_mount);
                        let target_rel = Self::to_usb_relative(&target_abs, usb_mount);
                        let expected_hash = compute_file_sha256(&file.source_path)?;

                        if let Some(journal_mgr) = move_journal.as_mut() {
                            journal_mgr.ensure_move_transaction(source_rel, target_rel, expected_hash)?;
                        }
                        move_candidates += 1;
                    }
                }
            }

            let provision_candidates = audio_files.len().saturating_sub(move_candidates);
            self.reporter.info("\n=== R-33 Topology Plan ===");
            self.reporter
                .info(&format!("[SKIP] {} files already compliant", skipped_existing));
            self.reporter
                .info(&format!("[MOVE] {} files for in-place reindex", move_candidates));
            self.reporter.info(&format!(
                "[PROVISION] {} files need encode/copy",
                provision_candidates
            ));
            self.reporter.info(&format!(
                "[QUARANTINE] {} orphan/untracked files",
                untracked_in_target.len()
            ));
        }

        if !dry_run {
            self.reporter
                .info("\nStep 5: Executing Physical Normalization & Copy...");

            let mut global_idx = 0;
            let mut skipped_drm = 0usize;
            let mut skipped_failed = 0usize;
            let mut processed_manifest = manifest::ProcessedFileManifest::load_or_create(usb_mount)?;
            self.reporter.start_progress(audio_files.len() as u64)?;

            for volume in volumes {
                checkpoint.add_volume(volume.folder_name.clone())?;
                let volume_dir = validate_path_containment(usb_mount, Path::new(&volume.folder_name))
                    .with_context(|| {
                        format!(
                            "R-05: Violacion de contencion en volumen: {}",
                            volume.folder_name
                        )
                    })?;

                fs::create_dir_all(&volume_dir)
                    .with_context(|| format!("Failed to create volume {}", volume_dir.display()))?;

                for file in volume.files {
                    let dest = validate_path_containment(&volume_dir, Path::new(&file.sanitized_name))
                        .with_context(|| {
                            format!(
                                "R-05: Violacion de contencion en destino: {}",
                                file.sanitized_name
                            )
                        })?;
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
                    // Evita forzar persistencia en USB por cada archivo.

                    let progress_msg = format!(
                        "Normalizing: {} -> {}",
                        volume.folder_name, file.sanitized_name
                    );

                    let is_mp3_source = file
                        .source_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("mp3"))
                        .unwrap_or(false);
                    let processing_decision = normalizer::classify_audio_processing(&file.source_path)?;

                    let mut used_in_place_move = false;
                    if sync_mode && is_mp3_source {
                        if let Some(displaced_usb_path) = displaced_in_target.get(&file.source_path) {
                            if processing_decision != normalizer::ProcessingDecision::FastInPlaceRename {
                                // Archivo sucio/incompatible: cae al pipeline de normalizacion.
                            } else {
                            let target_rel = Self::to_usb_relative(&dest, usb_mount);

                            if let Some(journal_mgr) = move_journal.as_ref() {
                                if matches!(
                                    journal_mgr.status_for_target(&target_rel),
                                    Some(journal::TransactionStatus::Committed)
                                ) && dest.exists()
                                {
                                    let usb_checksum = compute_file_sha256(&dest)?;
                                    checkpoint.mark_file_completed(global_idx, usb_checksum.clone())?;
                                    let usb_relative = Self::to_usb_relative(&dest, usb_mount)
                                        .to_string_lossy()
                                        .to_string();
                                    processed_manifest.register_processed_file(
                                        file.sanitized_name.clone(),
                                        usb_checksum,
                                        fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
                                        usb_relative,
                                        global_idx,
                                    );

                                    self.reporter.inc_progress(1, &progress_msg);
                                    used_in_place_move = true;
                                    self.reporter.info(&format!(
                                        "[MOVE-RESUME] transaction already committed: {}",
                                        dest.display()
                                    ));
                                }
                            }

                            if !used_in_place_move && displaced_usb_path.exists() {
                                if let Some(parent) = dest.parent() {
                                    fs::create_dir_all(parent)?;
                                }

                                if let Some(journal_mgr) = move_journal.as_mut() {
                                    journal_mgr.mark_in_progress(&target_rel)?;
                                }

                                fs::rename(displaced_usb_path, &dest).with_context(|| {
                                    format!(
                                        "Failed in-place move {} -> {}",
                                        displaced_usb_path.display(),
                                        dest.display()
                                    )
                                })?;

                                let expected_hash = compute_file_sha256(&file.source_path)?;
                                let moved_hash = compute_file_sha256(&dest)?;
                                if moved_hash != expected_hash {
                                    if let Some(journal_mgr) = move_journal.as_mut() {
                                        let _ = journal_mgr.mark_failed(
                                            &target_rel,
                                            "Hash mismatch after in-place move".to_string(),
                                        );
                                    }
                                    checkpoint.mark_file_failed(
                                        global_idx,
                                        "Hash mismatch after in-place move".to_string(),
                                    )?;
                                    return Err(anyhow::anyhow!(
                                        "Integrity error after in-place move on {}",
                                        dest.display()
                                    ));
                                }

                                if let Some(journal_mgr) = move_journal.as_mut() {
                                    journal_mgr.mark_committed(&target_rel)?;
                                }

                                checkpoint.mark_file_completed(global_idx, moved_hash.clone())?;
                                let usb_relative = Self::to_usb_relative(&dest, usb_mount)
                                    .to_string_lossy()
                                    .to_string();
                                processed_manifest.register_processed_file(
                                    file.sanitized_name.clone(),
                                    moved_hash,
                                    fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
                                    usb_relative,
                                    global_idx,
                                );

                                self.reporter.inc_progress(1, &progress_msg);
                                used_in_place_move = true;
                                self.reporter.info(&format!(
                                    "[MOVE] {} -> {}",
                                    displaced_usb_path.display(),
                                    dest.display()
                                ));
                            }
                            }
                        }
                    }

                    if used_in_place_move {
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
                        .emit(self.json_mode);

                        global_idx += 1;
                        continue;
                    }

                    match normalizer::normalize_audio(&file.source_path, &dest, processing_decision) {
                        Ok(_) => {
                            let usb_checksum = compute_file_sha256(&dest)?;
                            checkpoint.mark_file_completed(global_idx, usb_checksum.clone())?;
                            let usb_relative = Self::to_usb_relative(&dest, usb_mount)
                                .to_string_lossy()
                                .to_string();
                            processed_manifest.register_processed_file(
                                file.sanitized_name.clone(),
                                usb_checksum,
                                fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
                                usb_relative,
                                global_idx,
                            );
                            let files_processed = global_idx + 1;
                            self.reporter.inc_progress(1, &progress_msg);
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
                            .emit(self.json_mode);
                        }
                        Err(e) => {
                            if matches!(
                                e.downcast_ref::<ProvisioningError>(),
                                Some(ProvisioningError::DrmProtected { .. })
                            ) {
                                checkpoint.mark_file_failed(global_idx, "Skipped_DRM".to_string())?;
                                skipped_drm += 1;
                                if let Some(backup_meta) = backup.as_ref() {
                                    Self::append_drm_skip_log(&backup_meta.backup_dir, &file.source_path);
                                }
                                self.reporter.info(&format!(
                                    "[SKIP DRM] {}",
                                    file.source_path.to_string_lossy()
                                ));
                                IpcEvent::Warning {
                                    code: "DRM_SKIPPED".to_string(),
                                    source_file: file.source_path.to_string_lossy().to_string(),
                                    message:
                                        "El archivo esta protegido por cifrado DRM y fue ignorado."
                                            .to_string(),
                                }
                                .emit(self.json_mode);

                                self.reporter.inc_progress(1, &progress_msg);
                                global_idx += 1;
                                continue;
                            }

                            checkpoint.mark_file_failed(global_idx, e.to_string())?;

                            skipped_failed += 1;
                            IpcEvent::Warning {
                                code: "NORMALIZATION_FAILED".to_string(),
                                source_file: file.source_path.to_string_lossy().to_string(),
                                message: format!("Fallo de normalizacion, archivo omitido: {}", e),
                            }
                            .emit(self.json_mode);

                            self.reporter.info(&format!(
                                "[SKIP FAIL] {} -> {}",
                                file.source_path.display(),
                                e
                            ));
                            self.reporter.inc_progress(1, &progress_msg);
                            global_idx += 1;
                            continue;
                        }
                    }
                    global_idx += 1;
                }

                // Guardar manifest cada volumen completado para persistencia incremental
                processed_manifest.save_to_usb(usb_mount)?;

                // [R-02-010] Mitigacion de Desgaste NAND / Optimizacion I/O
                // Politica: no forzar sync por cada archivo; se consolida flush al cierre de volumen
                // y en eventos transaccionales (checkpoint/espejo/eject seguro).
                if let Ok(dir_file) = fs::File::open(&volume_dir) {
                    let _ = dir_file.sync_all();
                }

                // Consolidar persistencia del checkpoint al cierre de cada volumen.
                checkpoint.save_to_disk()?;
                if sync_mode {
                    Self::mirror_checkpoint_to_usb(&checkpoint, usb_mount)?;
                }
            }
            self.reporter
                .finish("Physical distribution and normalization completed.");
            self.reporter.info("Physical distribution complete.");

            if sync_mode {
                let removed_dirs = Self::prune_empty_non_compliant_root_dirs(usb_mount)?;
                if removed_dirs > 0 {
                    self.reporter.info(&format!(
                        "Limpieza topologica post-move: {} carpeta(s) raiz no estandar vacia(s) eliminada(s).",
                        removed_dirs
                    ));
                }
            }

            self.reporter
                .info("\nStep 6: Hardware Invariant Verification...");
            let checkpoint_data = checkpoint.get_data();
            let report =
                verification::pre_eject_verification(usb_mount, checkpoint_data, self.json_mode)?;

            if !report.success {
                return Err(anyhow::anyhow!(
                    "Provisioning failed final QA. Check logs for details."
                ));
            }

            checkpoint.finalize()?;
            if sync_mode {
                Self::mirror_checkpoint_to_usb(&checkpoint, usb_mount)?;
            }
            self.reporter.info("Checkpoint finalized after QA.");

            if !dry_run {
                self.reporter.info("\nStep 7: Safe Ejection...");
                verification::safe_eject(&device.device_path, usb_mount)?;
            }

            if sync_mode {
                if let Some(journal_mgr) = move_journal.as_ref() {
                    if journal_mgr.all_committed() {
                        journal::JournalManager::clear_from_usb(usb_mount)?;
                        self.reporter
                            .info("R-33 journal completado y limpiado de la USB.");
                    }
                }
            }

            IpcEvent::Success {
                total_processed: checkpoint
                    .get_data()
                    .processed_files
                    .values()
                    .filter(|f| f.status == checkpoint::OperationStatus::Completed)
                    .count(),
                total_skipped: skipped_drm + skipped_failed,
                elapsed_time_seconds: start.elapsed().as_secs(),
                message: format!(
                    "Provision completada y dispositivo desmontado de forma segura. {} archivo(s) aislado(s) en .legacy_quarantine (topologia:{} + huérfanos:{}).",
                    topology_quarantined_count + quarantined_count,
                    topology_quarantined_count,
                    quarantined_count
                ),
            }
            .emit(self.json_mode);
        }

        self.reporter.info("\n=== Provisioning Complete ===");
        Ok(())
    }

    pub fn resume_provisioning(&mut self, backup_dir: &Path, usb_mount: &Path) -> Result<()> {
        self.reporter.info("\n=== Resuming USB Provisioning ===");
        self.reporter
            .info(&format!("Backup Directory: {}", backup_dir.display()));
        self.reporter
            .info(&format!("USB Target: {}", usb_mount.display()));

        self.reporter.info("Step 1: Validating USB device...");
        let device = hardware::validate_device_path(usb_mount)?;
        device.is_valid_for_provisioning()?;

        // [R-02-009] Sonda de salud a nivel de controlador en recuperación
        hardware::assert_hardware_health(&device.device_path)?;

        let _lock = hardware::ProvisioningLock::acquire(usb_mount)?;
        hardware::assert_rw_filesystem(usb_mount)?;

        self.reporter.info(&format!(
            "USB device validated (RW & Locked): {}",
            usb_mount.display()
        ));

        let checkpoint_file = backup_dir.join(".provisioning_checkpoint");
        if !checkpoint_file.exists() {
            return Err(anyhow::anyhow!("No se encontro archivo de checkpoint."));
        }

        let mut checkpoint_mgr = checkpoint::CheckpointManager::load_from_disk(&checkpoint_file)?;

        if !checkpoint_mgr.get_data().is_recoverable() {
            self.reporter
                .info("La sesion registrada ya esta completada o no es recuperable.");
            return Ok(());
        }

        self.reporter.info(&format!(
            "Progreso anterior: {:.1}%",
            checkpoint_mgr.get_data().progress_percentage()
        ));

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
        .emit(self.json_mode);

        self.reporter.info("\n=== Recovery Complete ===");
        Ok(())
    }

    fn append_drm_skip_log(backup_dir: &Path, original_path: &Path) {
        let log_path = backup_dir.join("unsupported_drm_files.log");
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let _ = writeln!(file, "{}", original_path.display());
        }
    }

    fn to_usb_relative(path: &Path, usb_mount: &Path) -> PathBuf {
        path.strip_prefix(usb_mount)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| path.to_path_buf())
    }

    fn total_path_bytes(path: &Path) -> u64 {
        if path.is_file() {
            return fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        }

        if !path.is_dir() {
            return 0;
        }

        let mut total = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                total = total.saturating_add(Self::total_path_bytes(&entry.path()));
            }
        }
        total
    }

    fn prune_empty_non_compliant_root_dirs(usb_mount: &Path) -> Result<usize> {
        let mut removed = 0usize;

        fn remove_dir_if_empty(path: &Path) -> Result<bool> {
            match fs::read_dir(path) {
                Ok(mut entries) => {
                    if entries.next().is_none() {
                        match fs::remove_dir(path) {
                            Ok(_) => Ok(true),
                            Err(_) => Ok(false),
                        }
                    } else {
                        Ok(false)
                    }
                }
                Err(_) => Ok(false),
            }
        }

        for entry in fs::read_dir(usb_mount)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            if !path.is_dir() {
                continue;
            }

            if file_name.starts_with('.') || file_name.starts_with("System Volume") {
                continue;
            }

            if file_name.starts_with("VOL_") {
                continue;
            }

            if remove_dir_if_empty(&path)? {
                removed += 1;
            }
        }

        Ok(removed)
    }

    fn mirror_checkpoint_to_usb(
        checkpoint_mgr: &checkpoint::CheckpointManager,
        usb_mount: &Path,
    ) -> Result<()> {
        let checkpoint_path =
            validate_path_containment(usb_mount, Path::new(".provisioning_checkpoint"))?;
        let tmp_path =
            validate_path_containment(usb_mount, Path::new(".provisioning_checkpoint.tmp"))?;
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

    fn backup_usb_tree(
        backup_meta: &mut backup::BackupMetadata,
        root_path: &Path,
        current_path: &Path,
    ) -> Result<usize> {
        let file_name = current_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if matches!(file_name, ".lap_provisioning.lock" | ".fat32_dirty_test") {
            return Ok(0);
        }

        if current_path.is_file() {
            backup_meta.backup_file_preserving_tree(current_path, root_path)?;
            return Ok(1);
        }

        if !current_path.is_dir() {
            return Ok(0);
        }

        let mut total = 0usize;
        for entry in fs::read_dir(current_path)? {
            let entry = entry?;
            total += Self::backup_usb_tree(backup_meta, root_path, &entry.path())?;
        }

        Ok(total)
    }
}
