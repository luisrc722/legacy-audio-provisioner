//! R-10 & R-11: Normalización de Media y Stripping de Metadatos
//!
//! Dependencias del sistema operativo:
//! - ffprobe: Para análisis no destructivo de perfiles de audio.
//! - ffmpeg: Para transcodificación y remoción agresiva de bloques ID3v2/Video.

use anyhow::{anyhow, Context, Result};
use crate::error::ProvisioningError;
use log::{debug, info};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Deserialize)]
struct FfprobeOutput {
    streams: Vec<StreamInfo>,
}

#[derive(Deserialize)]
struct StreamInfo {
    codec_name: Option<String>,
    bit_rate: Option<String>,
    sample_rate: Option<String>,
}

pub struct AudioProfile {
    pub codec: String,
    pub bitrate: u32,
    pub sample_rate: u32,
}

/// R-19: Deteccion estricta de DRM (Apple FairPlay, WMA Protected, etc.).
pub fn detect_drm(path_str: &str) -> Result<bool> {
    // 1. Deteccion rapida por extension (Fail-Fast)
    if path_str.to_lowercase().ends_with(".m4p") {
        return Ok(true);
    }

    // 2. Inspeccion de tags de formato/streams con ffprobe
    let probe_output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags:stream_tags",
            "-of",
            "json",
            path_str,
        ])
        .output()
        .context("Fallo al ejecutar ffprobe para analisis de DRM")?;

    let output_str = String::from_utf8_lossy(&probe_output.stdout).to_lowercase();
    let err_str = String::from_utf8_lossy(&probe_output.stderr).to_lowercase();

    // Firmas comunes de encriptacion reportadas en metadata o errores del parser.
    let is_encrypted = output_str.contains("drms")
        || output_str.contains("drm_")
        || output_str.contains("fairplay")
        || err_str.contains("encrypted");

    Ok(is_encrypted)
}

/// Extrae la metadata técnica del archivo utilizando ffprobe.
pub fn analyze_audio(input: &Path) -> Result<AudioProfile> {
    let path_str = input.to_string_lossy();
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-select_streams",
            "a:0", // Crítico: ignora streams de video/carátulas
            path_str.as_ref(),
        ])
        .output()
        .context("Fallo al ejecutar ffprobe. ¿Está instalado en el sistema?")?;

    if !output.status.success() {
        return Err(anyhow!("ffprobe falló al analizar {}", input.display()));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let parsed: FfprobeOutput =
        serde_json::from_str(&json_str).context("Fallo al parsear el output JSON de ffprobe")?;

    let stream = parsed
        .streams
        .first()
        .ok_or_else(|| anyhow!("No hay stream de audio válido"))?;

    let codec = stream
        .codec_name
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let bitrate = stream
        .bit_rate
        .as_ref()
        .and_then(|b| b.parse::<u32>().ok())
        .unwrap_or(0);
    let sample_rate = stream
        .sample_rate
        .as_ref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    Ok(AudioProfile {
        codec,
        bitrate,
        sample_rate,
    })
}

/// Normaliza físicamente el archivo de audio.
/// Decide dinámicamente entre 'Passthrough' (copia rápida) o 'Transcodificación'.
pub fn normalize_audio(input: &Path, output: &Path) -> Result<()> {
    let path_str = input.to_string_lossy();
    if detect_drm(path_str.as_ref())? {
        return Err(anyhow::Error::new(ProvisioningError::DrmProtected {
            details: input.display().to_string(),
        }));
    }

    let profile = analyze_audio(input)?;

    // R-10: Estrategia de Passthrough Seguro
    // Rango de tolerancia de bitrate CBR seguro para estéreos legacy: 120k a 195k
    let is_safe_mp3 = profile.codec == "mp3"
        && profile.sample_rate == 44100
        && (profile.bitrate >= 120_000 && profile.bitrate <= 195_000);

    let mut cmd = Command::new("ffmpeg");

    // Argumentos base de limpieza (R-11)
    cmd.arg("-y") // Sobrescribir sin preguntar
        .arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:a:0") // Forzar extracción única de audio (elimina carátulas)
        .arg("-map_metadata")
        .arg("-1"); // Destruir todas las etiquetas ID3v2/metadatos

    if is_safe_mp3 {
        debug!(
            "Passthrough viable para {}. Copiando stream sin recodificar.",
            input.display()
        );
        cmd.arg("-c:a").arg("copy");
    } else {
        debug!(
            "Transcodificación requerida para {} (Codec: {}, BR: {}, SR: {}).",
            input.display(),
            profile.codec,
            profile.bitrate,
            profile.sample_rate
        );
        cmd.arg("-c:a")
            .arg("libmp3lame")
            .arg("-b:a")
            .arg("128k") // Forzar CBR a 128kbps (estándar legacy)
            .arg("-ar")
            .arg("44100"); // Sample rate estándar CD
    }

    cmd.arg(output);

    let cmd_output = cmd.output().context("Fallo en la ejecución de ffmpeg")?;

    if !cmd_output.status.success() {
        let stderr = String::from_utf8_lossy(&cmd_output.stderr);
        return Err(anyhow!("FFmpeg falló en {}: {}", input.display(), stderr));
    }

    info!("Normalizado: {}", output.display());
    Ok(())
}
