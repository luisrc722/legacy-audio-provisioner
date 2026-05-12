use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use lap_core::ingestion;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "lap-bin-ingest", version, about = "Ingesta USB -> staging")]
struct Args {
    #[arg(long)]
    usb: PathBuf,

    #[arg(long)]
    staging: PathBuf,

    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Serialize)]
struct IngestJsonOutput {
    status: &'static str,
    files_ingested: usize,
    total_bytes: u64,
    staging_dir: String,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let mut progress_bar: Option<ProgressBar> = None;

    let manifest = ingestion::ingest_audio_files_with_progress(
        &args.usb,
        &args.staging,
        args.json,
        |processed, total, current_file| {
            if args.json {
                return;
            }

            let pb = progress_bar.get_or_insert_with(|| {
                let pb = ProgressBar::new(total as u64);
                pb.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                    )
                    .expect("valid progress template")
                    .progress_chars("##-"),
                );
                pb
            });

            pb.set_message(format!("Procesando {}/{}: {}", processed, total, current_file));
            pb.set_position(processed as u64);

            if processed == total {
                pb.finish_with_message("Adecuamiento completado.");
            }
        },
    )
        .with_context(|| {
            format!(
                "Ingest failed from '{}' to '{}'",
                args.usb.display(),
                args.staging.display()
            )
        })?;

    if args.json {
        let out = IngestJsonOutput {
            status: "success",
            files_ingested: manifest.files.len(),
            total_bytes: manifest.total_bytes,
            staging_dir: manifest.staging_dir.display().to_string(),
        };
        println!("{}", serde_json::to_string(&out)?);
    } else {
        println!(
            "Ingest complete: {} files, {:.2} MB",
            manifest.files.len(),
            manifest.total_bytes as f64 / 1_048_576.0
        );
    }

    Ok(())
}
