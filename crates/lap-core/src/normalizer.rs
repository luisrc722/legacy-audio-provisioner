//! R-10 & R-11: Normalización de Media y Stripping de Metadatos
//!
//! Dependencias del sistema operativo:
//! - ffprobe: Para análisis no destructivo de perfiles de audio.
//! - ffmpeg: Para transcodificación y remoción agresiva de bloques ID3v2/Video.

use crate::error::ProvisioningError;
use crate::security::validate_shell_safe_filename;
use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Deserialize)]
struct FfprobeOutput {
    streams: Vec<StreamInfo>,
    format: Option<FormatInfo>,
}

#[derive(Deserialize)]
struct StreamInfo {
    codec_type: Option<String>,
    codec_name: Option<String>,
    bit_rate: Option<String>,
    sample_rate: Option<String>,
    disposition: Option<StreamDisposition>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct StreamDisposition {
    attached_pic: Option<u8>,
}

#[derive(Deserialize)]
struct FormatInfo {
    tags: Option<HashMap<String, String>>,
}

pub struct AudioProfile {
    pub codec: String,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub has_embedded_cover: bool,
    pub has_video_stream: bool,
    pub has_metadata_tags: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingDecision {
    FastInPlaceRename,
    FfmpegCopyClean,
    FfmpegTranscode,
}

fn classify_from_profile(profile: &AudioProfile) -> ProcessingDecision {
    let is_safe_mp3 = profile.codec == "mp3"
        && profile.sample_rate == 44100
        && (profile.bitrate >= 120_000 && profile.bitrate <= 320_000);

    let needs_cleanup =
        profile.has_embedded_cover || profile.has_video_stream || profile.has_metadata_tags;

    if is_safe_mp3 {
        if needs_cleanup {
            ProcessingDecision::FfmpegCopyClean
        } else {
            ProcessingDecision::FastInPlaceRename
        }
    } else {
        ProcessingDecision::FfmpegTranscode
    }
}

fn has_embedded_cover_art(parsed: &FfprobeOutput) -> bool {
    for stream in &parsed.streams {
        if stream
            .disposition
            .as_ref()
            .and_then(|d| d.attached_pic)
            .unwrap_or(0)
            == 1
        {
            return true;
        }

        if let Some(tags) = &stream.tags {
            if tags
                .keys()
                .any(|k| matches!(k.to_ascii_lowercase().as_str(), "apic" | "covr" | "metadata_block_picture"))
            {
                return true;
            }
        }
    }

    if let Some(format_info) = &parsed.format {
        if let Some(tags) = &format_info.tags {
            if tags
                .keys()
                .any(|k| matches!(k.to_ascii_lowercase().as_str(), "apic" | "covr" | "metadata_block_picture"))
            {
                return true;
            }
        }
    }

    false
}

fn has_video_stream(parsed: &FfprobeOutput) -> bool {
    parsed
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("video"))
}

fn has_non_empty_metadata_tags(parsed: &FfprobeOutput) -> bool {
    if parsed
        .streams
        .iter()
        .any(|s| s.tags.as_ref().map(|t| !t.is_empty()).unwrap_or(false))
    {
        return true;
    }

    parsed
        .format
        .as_ref()
        .and_then(|f| f.tags.as_ref())
        .map(|t| !t.is_empty())
        .unwrap_or(false)
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
            "-show_format",
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

    let has_embedded_cover = has_embedded_cover_art(&parsed);
    let has_video_stream = has_video_stream(&parsed);
    let has_metadata_tags = has_non_empty_metadata_tags(&parsed);

    let stream = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"))
        .or_else(|| parsed.streams.first())
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
        has_embedded_cover,
        has_video_stream,
        has_metadata_tags,
    })
}

pub fn classify_audio_processing(input: &Path) -> Result<ProcessingDecision> {
    let profile = analyze_audio(input)?;
    Ok(classify_from_profile(&profile))
}

/// Normaliza físicamente el archivo de audio.
/// Debe invocarse solo cuando la decisión NO es `FastInPlaceRename`.
pub fn normalize_audio(input: &Path, output: &Path, decision: ProcessingDecision) -> Result<()> {
    // R-35 debe aplicarse al nombre mutado/saneado que controlamos como destino.
    // El path fuente se pasa a ffprobe/ffmpeg como argv directo, sin shell intermedio.
    let output_filename = output.file_name().and_then(|n| n.to_str()).unwrap_or("");

    validate_shell_safe_filename(output_filename).with_context(|| {
        format!(
            "Output filename contains shell injection characters (R-35): {}",
            output_filename
        )
    })?;

    if decision == ProcessingDecision::FastInPlaceRename {
        return Err(anyhow!(
            "normalize_audio fue invocado con FastInPlaceRename; use fs::rename en el orquestador"
        ));
    }

    let path_str = input.to_string_lossy();
    if detect_drm(path_str.as_ref())? {
        return Err(anyhow::Error::new(ProvisioningError::DrmProtected {
            details: input.display().to_string(),
        }));
    }

    let mut cmd = Command::new("ffmpeg");

    // Argumentos base de limpieza (R-11)
    cmd.arg("-y") // Sobrescribir sin preguntar
        .arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:a:0") // Forzar extracción única de audio (elimina carátulas)
        .arg("-map_metadata")
        .arg("-1"); // Destruir todas las etiquetas ID3v2/metadatos

    match decision {
        ProcessingDecision::FfmpegCopyClean => {
            debug!(
                "Passthrough via ffmpeg copy para {} (limpieza de stream/tag).",
                input.display()
            );
            cmd.arg("-c:a").arg("copy");
        }
        ProcessingDecision::FfmpegTranscode => {
            debug!("Transcodificación requerida para {}.", input.display());
            cmd.arg("-c:a")
                .arg("libmp3lame")
                .arg("-b:a")
                .arg("128k")
                .arg("-ar")
                .arg("44100")
                .arg("-ac")
                .arg("2"); // Forzar stereo para compatibilidad con firmware legacy
        }
        ProcessingDecision::FastInPlaceRename => unreachable!(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cover_art_from_attached_pic_stream() {
        let parsed = FfprobeOutput {
            streams: vec![StreamInfo {
                codec_type: Some("video".to_string()),
                codec_name: Some("mjpeg".to_string()),
                bit_rate: None,
                sample_rate: None,
                disposition: Some(StreamDisposition {
                    attached_pic: Some(1),
                }),
                tags: None,
            }],
            format: None,
        };

        assert!(has_embedded_cover_art(&parsed));
    }

    #[test]
    fn test_detect_cover_art_from_apic_tag() {
        let mut tags = HashMap::new();
        tags.insert("APIC".to_string(), "cover data".to_string());

        let parsed = FfprobeOutput {
            streams: vec![StreamInfo {
                codec_type: Some("audio".to_string()),
                codec_name: Some("mp3".to_string()),
                bit_rate: Some("128000".to_string()),
                sample_rate: Some("44100".to_string()),
                disposition: None,
                tags: Some(tags),
            }],
            format: None,
        };

        assert!(has_embedded_cover_art(&parsed));
    }

    #[test]
    fn test_detect_cover_art_returns_false_when_absent() {
        let parsed = FfprobeOutput {
            streams: vec![StreamInfo {
                codec_type: Some("audio".to_string()),
                codec_name: Some("mp3".to_string()),
                bit_rate: Some("128000".to_string()),
                sample_rate: Some("44100".to_string()),
                disposition: Some(StreamDisposition {
                    attached_pic: Some(0),
                }),
                tags: None,
            }],
            format: None,
        };

        assert!(!has_embedded_cover_art(&parsed));
    }

    #[test]
    fn test_detects_video_stream() {
        let parsed = FfprobeOutput {
            streams: vec![StreamInfo {
                codec_type: Some("video".to_string()),
                codec_name: Some("mjpeg".to_string()),
                bit_rate: None,
                sample_rate: None,
                disposition: None,
                tags: None,
            }],
            format: None,
        };

        assert!(has_video_stream(&parsed));
    }

    #[test]
    fn test_detects_non_empty_metadata_tags() {
        let mut tags = HashMap::new();
        tags.insert("TITLE".to_string(), "demo".to_string());

        let parsed = FfprobeOutput {
            streams: vec![StreamInfo {
                codec_type: Some("audio".to_string()),
                codec_name: Some("mp3".to_string()),
                bit_rate: Some("128000".to_string()),
                sample_rate: Some("44100".to_string()),
                disposition: None,
                tags: Some(tags),
            }],
            format: None,
        };

        assert!(has_non_empty_metadata_tags(&parsed));
    }
}
