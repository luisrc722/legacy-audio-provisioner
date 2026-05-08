use anyhow::{Context, Result};
use clap::Parser;
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

    let manifest = ingestion::ingest_audio_files(&args.usb, &args.staging, args.json)
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
