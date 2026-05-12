use any_ascii::any_ascii;
use log::warn;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

// R-03: Sanitización de Nombres
// Requisitos:
// - Transliteration ASCII antes de filtrar
// - Stem legacy maximo de 64 caracteres
// - Extension preservada en minusculas cuando existe
const LEGACY_MAX_FILENAME_BYTES: usize = 32;
const LEGACY_MAX_STEM_CHARS: usize = 64;
const HASH_SUFFIX_HEX_LEN: usize = 8;

fn leading_symbol_junk_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^[\s+\-*_]+")
            .expect("valid leading symbols/spaces regex")
    })
}

fn leading_index_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d+(?:[\s+\-*_]+)+")
            .expect("valid leading numeric-index regex")
    })
}

fn leading_ad_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^\s*\(?\s*audiomovil(?:[._\s-]*\d{4})\s*\)?(?:[\s+\-*_]+)?")
            .expect("valid leading ad-tag regex")
    })
}

fn trailing_ad_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:[-_\s]*\(?\s*audiomovil(?:[._\s-]*\d{4})?(?:\s*\(\d+\))?\s*\)?|\s*\[\s*safari music\s*\])$",
        )
        .expect("valid trailing ad tag regex")
    })
}

fn trailing_noise_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[\s._\-\(\)\[\]]+$").expect("valid trailing noise regex")
    })
}

fn separator_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[^a-z0-9]+").expect("valid separator regex"))
}

fn underscore_run_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"_+").expect("valid underscore regex"))
}

fn audio_extension_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?i:mp3|flac|wav|ogg|m4a|alac|aac|wma|opus|aiff)$")
            .expect("valid audio extension regex")
    })
}

fn split_audio_extension(input: &str) -> (String, Option<String>) {
    if let Some((stem, ext)) = input.rsplit_once('.') {
        if !stem.is_empty() && audio_extension_re().is_match(ext) {
            return (stem.to_string(), Some(ext.to_ascii_lowercase()));
        }
    }

    (input.to_string(), None)
}

fn sanitize_stem(input: &str) -> String {
    let mut current = input.to_string();

    loop {
        let mut changed = false;

        let stripped = leading_symbol_junk_re().replace(&current, "").into_owned();
        if stripped != current {
            current = stripped;
            changed = true;
        }

        let stripped = leading_index_re().replace(&current, "").into_owned();
        if stripped != current {
            if stripped.trim().is_empty() {
                // Si el nombre es solo numerico, lo conservamos.
            } else {
                current = stripped;
                changed = true;
            }
        }

        let stripped = leading_ad_tag_re().replace(&current, "").into_owned();
        if stripped != current {
            current = stripped;
            changed = true;
        }

        let stripped = trailing_ad_tag_re().replace(&current, "").into_owned();
        if stripped != current {
            current = stripped;
            changed = true;
        }

        let stripped = trailing_noise_re().replace(&current, "").into_owned();
        if stripped != current {
            current = stripped;
            changed = true;
        }

        let trimmed = current.trim().to_string();
        if trimmed != current {
            current = trimmed;
            changed = true;
        }

        if !changed {
            break;
        }
    }

    let mut normalized = separator_re().replace_all(&current, "_").into_owned();
    normalized = underscore_run_re().replace_all(&normalized, "_").into_owned();
    normalized = normalized.trim_matches('_').to_string();

    if normalized.is_empty() {
        normalized = "audio".to_string();
    }

    normalized.chars().take(LEGACY_MAX_STEM_CHARS).collect()
}

/// Sanitiza nombres de audio con transliteracion ASCII, limpieza de junk inicial/final,
/// normalizacion de separadores y preservacion de extension audio en minusculas.
///
/// # Ejemplo
/// ```
/// use lap_core::sanitizer::sanitize_filename;
///
/// let cleaned = sanitize_filename("Canción_2024_éxito🎵.mp3");
/// assert_eq!(cleaned, "cancion_2024_exito.mp3");
/// ```
pub fn sanitize_filename(input: &str) -> String {
    let transliterated = any_ascii(input).to_lowercase();
    let base_name = Path::new(&transliterated)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&transliterated);
    let (stem, extension) = split_audio_extension(base_name);

    let sanitized_stem = sanitize_stem(&stem);
    let result = match extension {
        Some(ext) if !ext.is_empty() => format!("{}.{}", sanitized_stem, ext),
        _ => sanitized_stem,
    };

    if result != input {
        warn!(
            "Filename sanitized (transliterated/normalized): '{}' → '{}'",
            input,
            result
        );
    }

    result
}

/// Añade el prefijo secuencial y asegura de forma inteligente el límite de 32 caracteres
/// garantizando que la extensión nunca se pierda.
///
/// # Ejemplo
/// ```
/// use lap_core::sanitizer::add_sequential_prefix;
///
/// let indexed = add_sequential_prefix("song.mp3", 1);
/// assert_eq!(indexed, "001_song.mp3");
/// ```
pub fn add_sequential_prefix(filename: &str, index: usize) -> String {
    let prefix = format!("{:03}_", index);
    let max_len = 32;

    let path = std::path::Path::new(filename);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    let ext_part = if ext.is_empty() {
        String::new()
    } else {
        format!(".{}", ext)
    };

    // Si prefijo + extensión ya exceden el límite físico, se aplica truncamiento absoluto.
    if prefix.len() + ext_part.len() > max_len {
        return format!("{}{}{}", prefix, stem, ext_part)
            .chars()
            .take(max_len)
            .collect();
    }

    // Caso normal: preservar extensión y truncar solo el stem.
    let available_stem_len = max_len - prefix.len() - ext_part.len();
    let truncated_stem: String = stem.chars().take(available_stem_len).collect();

    format!("{}{}{}", prefix, truncated_stem, ext_part)
}

/// Construye un nombre legacy compacto y determinista orientado a hardware con poca cache.
///
/// Formato: `NNN_<stem_truncado>_<hash8>.mp3`
/// - `NNN_`: indice secuencial (compatibilidad de orden)
/// - `<stem_truncado>`: nombre sanitizado y truncado por presupuesto de bytes
/// - `<hash8>`: primeros 8 hex del SHA256 de contenido (unicidad)
///
/// Invariante: salida ASCII y longitud == 32 bytes.
pub fn build_hashed_legacy_name(original_stem: &str, index: usize, sha256_hex: &str) -> String {
    let prefix = format!("{:03}_", index);
    let hash8 = hash8_from_sha256_hex(sha256_hex);
    let suffix = format!("_{}.mp3", hash8);

    if prefix.len() + suffix.len() >= LEGACY_MAX_FILENAME_BYTES {
        warn!(
            "Index prefix too large for strict legacy budget ({}): index={}.",
            LEGACY_MAX_FILENAME_BYTES,
            index
        );
        return format!("{}{}", prefix, suffix)
            .chars()
            .take(LEGACY_MAX_FILENAME_BYTES)
            .collect();
    }

    let cleaned = sanitize_filename(original_stem);
    let mut safe_stem = if cleaned.is_empty() {
        "audio".to_string()
    } else {
        cleaned
    };

    let available_stem_len = LEGACY_MAX_FILENAME_BYTES - prefix.len() - suffix.len();
    safe_stem = safe_stem.chars().take(available_stem_len).collect();
    if safe_stem.is_empty() {
        safe_stem = "a".to_string();
    }
    let safe_len = safe_stem.chars().count();
    if safe_len < available_stem_len {
        safe_stem.push_str(&"_".repeat(available_stem_len - safe_len));
    }

    format!("{}{}{}", prefix, safe_stem, suffix)
}

fn hash8_from_sha256_hex(input: &str) -> String {
    let normalized: String = input
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_lowercase())
        .take(HASH_SUFFIX_HEX_LEN)
        .collect();

    if normalized.len() == HASH_SUFFIX_HEX_LEN {
        normalized
    } else {
        "00000000".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_sanitize_basic() {
        assert_eq!(sanitize_filename("Canción.mp3"), "cancion.mp3");
        assert_eq!(sanitize_filename("track+remix=v2.mp3"), "track_remix_v2.mp3");
        assert_eq!(sanitize_filename("normal_file-01.mp3"), "normal_file_01.mp3");
        assert_eq!(sanitize_filename("emoji🎵.mp3"), "emoji_musical_note.mp3");
    }

    #[test]
    fn test_sanitize_transliterates_and_normalizes_complex_name() {
        let input = "1 NO ME ENGAÑES NUNCA - yulios kumbia FT. mexicolombialos, los telez-AUDIOMOVIL.2021 (1).mp3";
        let result = sanitize_filename(input);

        assert_eq!(result, "no_me_enganes_nunca_yulios_kumbia_ft_mexicolombialos_los_telez.mp3");
        assert!(result.len() <= 68);
        assert!(result.ends_with(".mp3"));
        assert!(result.is_ascii());
    }

    #[test]
    fn test_sanitize_removes_leading_symbol_and_numeric_noise_only_at_start() {
        assert_eq!(
            sanitize_filename("+++ 000 Audiomovil spot.mp3"),
            "audiomovil_spot.mp3"
        );
        assert_eq!(
            sanitize_filename("0 Calibre 50 - Mitad Y Mitad.mp3"),
            "calibre_50_mitad_y_mitad.mp3"
        );
    }

    #[test]
    fn test_sanitize_removes_audiomovil_tag_at_start_and_end_case_insensitive() {
        assert_eq!(
            sanitize_filename("AUDIOMOVIL.2021 Mi Track - aUdIoMoViL.2021.mp3"),
            "mi_track.mp3"
        );
    }

    #[test]
    fn test_sequential_prefix_protects_extension() {
        // Nombre de 40 chars + .mp3. Debería truncar el nombre pero mantener 001_ y .mp3
        let long_name = format!("{}.mp3", "a".repeat(40));
        let result = add_sequential_prefix(&long_name, 1);

        assert_eq!(result.len(), 32);
        assert!(result.starts_with("001_"));
        assert!(result.ends_with(".mp3"));
        // 001_ (4) + a repetida 24 veces (24) + .mp3 (4) = 32
        assert_eq!(result, format!("001_{}.mp3", "a".repeat(24)));
    }

    #[test]
    fn test_no_extension() {
        let result = add_sequential_prefix("archivo_sin_extension_muy_largo_de_verdad", 999);
        assert_eq!(result.len(), 32);
        assert_eq!(result, "999_archivo_sin_extension_muy_la");
    }

    #[test]
    fn test_basic_prefix() {
        let result = add_sequential_prefix("mysong.mp3", 1);
        assert_eq!(result, "001_mysong.mp3");
    }

    #[test]
    fn test_three_digit_prefix() {
        let result = add_sequential_prefix("track.flac", 999);
        assert_eq!(result, "999_track.flac");
    }

    #[test]
    fn test_extension_preservation_critical_case() {
        // Este es el "caso asesino" que encontró el bug:
        // Un nombre exacto donde sin protección la extensión se pierda
        let critical = "una_cancion_de_rock_muy_larga_y_buena.mp3"; // 43 chars
        let result = add_sequential_prefix(critical, 5);

        assert_eq!(result.len(), 32);
        assert!(result.ends_with(".mp3"), "La extensión debe estar presente");
        assert!(result.starts_with("005_"), "El prefijo debe estar presente");
    }

    #[test]
    fn test_hashed_legacy_name_is_strictly_bounded_and_deterministic() {
        let result = build_hashed_legacy_name(
            "Canción súper larga con símbolos 🎵",
            7,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );

        assert_eq!(result.len(), 32);
        assert!(result.starts_with("007_"));
        assert!(result.ends_with("_01234567.mp3"));
    }

    #[test]
    fn test_hashed_legacy_name_fallback_hash_when_invalid() {
        let result = build_hashed_legacy_name("track", 3, "not-a-valid-sha");

        assert_eq!(result, "003_track___________00000000.mp3");
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_hashed_legacy_name_removes_leading_track_index_artifacts() {
        let result = build_hashed_legacy_name(
            "01 - Cancion De Prueba",
            4,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );

        assert!(result.starts_with("004_cancion"), "resultado inesperado: {}", result);
        assert!(result.ends_with("_01234567.mp3"));
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_hashed_legacy_name_always_exact_32_bytes_for_short_stems() {
        let result = build_hashed_legacy_name(
            "x",
            8,
            "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd",
        );

        assert_eq!(result.len(), 32);
        assert!(result.starts_with("008_x"));
        assert!(result.ends_with("_abcdefab.mp3"));
    }

    #[test]
    fn test_sanitize_removes_ad_tags_and_keeps_mp3_lowercase() {
        assert_eq!(sanitize_filename("+++ 0000 AUDIOMOVIL INTROMIX-AUDIOMOVIL.2021.mp3"), "audiomovil_intromix.mp3");
        assert_eq!(sanitize_filename("(AUDIOMOVIL2019).mp3"), "audio.mp3");
    }

    #[test]
    fn test_sanitize_limits_stem_length_to_64_chars() {
        let result = sanitize_filename(&format!("{}.mp3", "a".repeat(100)));

        assert!(result.ends_with(".mp3"));
        assert_eq!(result.trim_end_matches(".mp3").len(), 64);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2000))]

        #[test]
        fn prop_sanitize_outputs_ascii_and_is_bounded(ref input in "\\PC*") {
            let result = sanitize_filename(input);

            prop_assert!(result.is_ascii());
            prop_assert!(result.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'));

            if let Some((stem, ext)) = result.rsplit_once('.') {
                prop_assert!(stem.len() <= LEGACY_MAX_STEM_CHARS);
                if !ext.is_empty() {
                    prop_assert!(matches!(ext, "mp3" | "flac" | "wav" | "ogg" | "m4a" | "alac" | "aac" | "wma" | "opus" | "aiff"));
                }
            } else {
                prop_assert!(result.len() <= LEGACY_MAX_STEM_CHARS);
            }
        }
    }
}
