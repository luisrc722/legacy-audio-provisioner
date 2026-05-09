use anyhow::{Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use log::info;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use lap_core::error::ProvisioningError;
use lap_core::ipc::IpcEvent;
use lap_core::state;

mod messages;
mod orchestrator;
mod reporter;

use messages::{init_locale, tr};
use orchestrator::ProvisioningOrchestrator;
use reporter::create_reporter;

static SESSION_LOGGER: OnceLock<Mutex<SessionLogger>> = OnceLock::new();

struct SessionLogger {
    session_id: String,
    log_path: PathBuf,
    file: fs::File,
}

/// [R-01-005] Logging Estructurado
/// Precondicion: existe un directorio de logs del host resoluble por entorno o fallback.
/// Postcondicion: se crea una sesion con `provisioning.log` en formato JSON-lines auditable.
/// Invariante: cada ejecucion registra eventos `SESSION_START` y `COMMAND_*` con `session_id` consistente.
fn init_session_logger() -> Result<PathBuf> {
    let base_dir = if let Ok(custom) = std::env::var("LAP_LOG_DIR") {
        PathBuf::from(custom)
    } else {
        state::state_root_dir()?.join("logs")
    };

    fs::create_dir_all(&base_dir).with_context(|| {
        format!(
            "No se pudo crear directorio de logs '{}'",
            base_dir.display()
        )
    })?;

    let session_id = format!(
        "session_{}_{}",
        Local::now().format("%Y%m%d_%H%M%S"),
        std::process::id()
    );
    let session_dir = base_dir.join(&session_id);
    fs::create_dir_all(&session_dir).with_context(|| {
        format!(
            "No se pudo crear directorio de sesion '{}'",
            session_dir.display()
        )
    })?;

    let log_path = session_dir.join("provisioning.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("No se pudo abrir log '{}'", log_path.display()))?;

    let logger = SessionLogger {
        session_id,
        log_path: log_path.clone(),
        file,
    };

    let _ = SESSION_LOGGER.set(Mutex::new(logger));

    log_session_event("SESSION_START", "OK", "Inicio de sesion de provisioning");

    Ok(log_path)
}

fn log_session_event(operation: &str, status: &str, message: &str) {
    let Some(shared_logger) = SESSION_LOGGER.get() else {
        return;
    };

    let Ok(mut logger) = shared_logger.lock() else {
        return;
    };

    let entry = serde_json::json!({
        "timestamp": Local::now().to_rfc3339(),
        "session_id": logger.session_id,
        "operation": operation,
        "status": status,
        "message": message,
    });

    let _ = writeln!(logger.file, "{}", entry);
}

fn session_log_path() -> Option<PathBuf> {
    let shared_logger = SESSION_LOGGER.get()?;
    let logger = shared_logger.lock().ok()?;
    Some(logger.log_path.clone())
}

/// [R-01-006] EntryPoint Delgada
/// Precondicion: los comandos CLI ya fueron parseados y validados por `clap`.
/// Postcondicion: el binario delega el flujo de negocio a `ProvisioningOrchestrator`.
/// Invariante: la logica operacional de provisionamiento no se implementa en el entrypoint.

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

    #[arg(
        long,
        value_name = "LANG",
        default_value = "es",
        value_parser = ["es", "en"],
        help = "Idioma de mensajes runtime: es|en",
        global = true
    )]
    lang: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Lista los dispositivos USB/extraibles detectados
    List,

    /// Escanea audio en una USB especifica o en la primera detectada
    Scan {
        #[arg(long, value_name = "PATH")]
        usb: Option<PathBuf>,
    },

    /// Procesa, normaliza y sincroniza audio hacia la USB
    Provision {
        #[arg(long, value_name = "PATH")]
        usb: PathBuf,

        #[arg(long, value_name = "PATH")]
        source: PathBuf,

        #[arg(long)]
        dry_run: bool,

        #[arg(long, help = "Modo incremental: solo procesa archivos nuevos por hash")]
        sync: bool,

        #[arg(
            long,
            help = "Reconstruye topologia y nombres in-place sobre la USB con renames de metadatos (sin ffmpeg, sin staging)"
        )]
        in_place_rebuild: bool,
    },

    /// Respalda y reformatea la USB a FAT32 con clúster de 32 KB para firmware legacy
    Format {
        #[arg(long, value_name = "PATH")]
        usb: PathBuf,

        #[arg(
            long,
            value_name = "DEVICE_PATH",
            help = "Confirmacion destructiva: debe coincidir exactamente con el dispositivo detectado, por ejemplo /dev/sdb1"
        )]
        confirm_device: String,

        #[arg(long, value_name = "LABEL", help = "Etiqueta FAT32 opcional (max 11 chars)")]
        label: Option<String>,

        #[arg(long, help = "Reformatea incluso si el volumen ya cumple el perfil legacy")]
        force_reformat: bool,
    },

    /// Reanuda una sesion interrumpida desde un backup
    Resume {
        #[arg(long, value_name = "PATH")]
        usb: PathBuf,

        #[arg(long, value_name = "BACKUP_DIR")]
        resume: PathBuf,
    },

    /// Copia solo audio desde origen hacia staging local del host (R-31)
    Ingest {
        #[arg(long, value_name = "PATH")]
        usb: PathBuf,

        #[arg(long, value_name = "PATH")]
        source: PathBuf,
    },

    /// Orquesta ingesta + provision --sync para refactorizacion in-situ (R-31)
    Refactor {
        #[arg(long, value_name = "PATH")]
        usb: PathBuf,

        #[arg(long, value_name = "PATH")]
        source: PathBuf,

        #[arg(long, help = "Conservar el staging local al finalizar")]
        keep_staging: bool,
    },
}

fn main() -> std::result::Result<(), ProvisioningError> {
    let cli = Cli::parse();
    init_locale(Some(&cli.lang));

    if let Ok(path) = init_session_logger() {
        eprintln!(
            "{}: {}",
            tr("Log de sesion", "Session log"),
            path.display()
        );
    }

    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    std::env::set_var("RUST_LOG", log_level);
    env_logger::init();

    info!("{}", tr("=== Legacy Audio Provisioner ===", "=== Legacy Audio Provisioner ==="));
    info!(
        "{} {} | Spec-Driven Development",
        tr("Version", "Version"),
        env!("CARGO_PKG_VERSION")
    );

    log_session_event("COMMAND_START", "OK", &format!("{:?}", cli.command));

    let mut orchestrator = ProvisioningOrchestrator::new(create_reporter(cli.json), cli.json);

    let execution_result: std::result::Result<(), ProvisioningError> = match cli.command {
        Commands::List => {
            if cli.json {
                Err(ProvisioningError::UnsupportedJsonMode {
                    feature: "list".to_string(),
                })
            } else {
                orchestrator
                    .list_usb_devices()
                    .map_err(ProvisioningError::from_anyhow)
            }
        }
        Commands::Scan { usb } => {
            if cli.json {
                Err(ProvisioningError::UnsupportedJsonMode {
                    feature: "scan".to_string(),
                })
            } else {
                orchestrator
                    .scan_usb_audio(usb.as_deref())
                    .map_err(ProvisioningError::from_anyhow)
            }
        }
        Commands::Resume { usb, resume } => orchestrator
            .resume_provisioning(&resume, &usb)
            .map_err(ProvisioningError::from_anyhow),
        Commands::Provision {
            usb,
            source,
            dry_run,
            sync,
            in_place_rebuild,
        } => {
            if in_place_rebuild {
                let usb_can = usb
                    .canonicalize()
                    .map_err(|e| ProvisioningError::InvalidConfig {
                        details: format!(
                            "{} '{}': {}",
                            tr("No se pudo resolver la ruta USB", "Could not resolve USB path"),
                            usb.display(),
                            e
                        ),
                    })?;
                let source_can = source
                    .canonicalize()
                    .map_err(|e| ProvisioningError::InvalidConfig {
                        details: format!(
                            "{} '{}': {}",
                            tr(
                                "No se pudo resolver la ruta de origen",
                                "Could not resolve source path"
                            ),
                            source.display(),
                            e
                        ),
                    })?;

                if usb_can != source_can {
                    return Err(ProvisioningError::InvalidConfig {
                        details: format!(
                            "{} ('{}' != '{}').",
                            tr(
                                "Con --in-place-rebuild, --source debe apuntar al mismo mount USB",
                                "With --in-place-rebuild, --source must point to the same USB mount"
                            ),
                            usb_can.display(),
                            source_can.display()
                        ),
                    });
                }
            } else {
                orchestrator.validate_canonical_paths(&usb, &source)?;
            }

            orchestrator
                .provision_usb(&usb, &source, dry_run, sync, in_place_rebuild)
                .map_err(ProvisioningError::from_anyhow)
        }
        Commands::Format {
            usb,
            confirm_device,
            label,
            force_reformat,
        } => orchestrator
            .format_usb_for_legacy(&usb, &confirm_device, label.as_deref(), force_reformat)
            .map_err(ProvisioningError::from_anyhow),
        Commands::Ingest { usb, source } => orchestrator
            .ingest_staging(&usb, &source)
            .map_err(ProvisioningError::from_anyhow),
        Commands::Refactor {
            usb,
            source,
            keep_staging,
        } => orchestrator
            .refactor_usb(&usb, &source, keep_staging)
            .map_err(ProvisioningError::from_anyhow),
    };

    if let Err(e) = execution_result {
        log_session_event(
            "COMMAND_END",
            "ERROR",
            &format!("{} | action: {}", e, e.action_required()),
        );
        IpcEvent::FatalError {
            code: e.code().to_string(),
            message: e.to_string(),
            action_required: e.action_required().to_string(),
        }
        .emit(cli.json);
        return Err(e);
    }

    if let Some(path) = session_log_path() {
        log_session_event(
            "COMMAND_END",
            "OK",
            &format!(
                "{}: {}",
                tr("Ejecucion completada. Log", "Execution completed. Log"),
                path.display()
            ),
        );
    } else {
        log_session_event(
            "COMMAND_END",
            "OK",
            tr("Ejecucion completada", "Execution completed"),
        );
    }

    Ok(())
}
