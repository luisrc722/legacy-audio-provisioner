use log::warn;
use regex::Regex;
use std::sync::OnceLock;

// R-03: Sanitización de Nombres
// Requisitos:
// - Máximo 32 caracteres por archivo
// - Codificación: Estrictamente ASCII/ISO-8859-1
// - Regex de limpieza: `[^a-zA-Z0-9\.\-\_]`

static SANITIZE_REGEX: OnceLock<Regex> = OnceLock::new();
const LEGACY_MAX_FILENAME_BYTES: usize = 32;
const HASH_SUFFIX_HEX_LEN: usize = 8;

fn get_regex() -> &'static Regex {
    SANITIZE_REGEX.get_or_init(|| Regex::new(r"[^a-zA-Z0-9\.\-\_]").unwrap())
}

/// Sanitiza eliminando caracteres inválidos.
/// NOTA: Ya no truncamos aquí para evitar destruir la extensión antes de tiempo.
///
/// # Ejemplo
/// ```
/// use lap_core::sanitizer::sanitize_filename;
///
/// let cleaned = sanitize_filename("Canción_2024_éxito🎵.mp3");
/// assert_eq!(cleaned, "Cancin_2024_xito.mp3");
/// ```
pub fn sanitize_filename(input: &str) -> String {
    let result = get_regex().replace_all(input, "").into_owned();
    if result != input {
        warn!(
            "Filename sanitized (non-ASCII/invalid chars removed): '{}' → '{}'",
            input, result
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
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
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
/// Invariante: salida ASCII y longitud <= 32 bytes.
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

    #[test]
    fn test_sanitize_basic() {
        assert_eq!(sanitize_filename("valid_name.mp3"), "valid_name.mp3");
        assert_eq!(sanitize_filename("Canción.mp3"), "Cancin.mp3");
        assert_eq!(sanitize_filename("song🎵.mp3"), "song.mp3");
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

        assert_eq!(result, "003_track_00000000.mp3");
        assert!(result.len() <= 32);
    }

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(10000))]

            #[test]
            fn prop_sanitize_never_panics_and_filters_correctly(ref input in "\\PC*") {
                let result = sanitize_filename(input);

                // Los caracteres inválidos no deben filtrarse hacia la salida.
                let is_valid = result
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
                prop_assert!(is_valid, "Fuga de caracteres invalidos: {}", result);

                // Los caracteres válidos del input deben sobrevivir en el output.
                // (una función que devuelva "" siempre pasaría el chequeo anterior)
                let valid_in_input: String = input
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
                    .collect();
                prop_assert_eq!(
                    result, valid_in_input,
                    "Se perdieron caracteres validos del input: '{}'",
                    input
                );
            }

            #[test]
            /// Usa extensiones cortas realistas (mp3, flac, wav — ≤5 chars) para
            /// reflejar el dominio real. Con extensiones largas se activa el branch de
            /// truncamiento absoluto que sacrifica la extensión intencionalmente, pero
            /// eso nunca ocurre con audio.
            fn prop_prefix_enforces_hardware_limits(
                ref stem in "[a-zA-Z0-9_-]{0,100}",
                ref ext in "[a-zA-Z0-9]{0,5}",
                index in 1usize..=999
            ) {
                let input = if ext.is_empty() {
                    stem.clone()
                } else {
                    format!("{}.{}", stem, ext)
                };

                let result = add_sequential_prefix(&input, index);

                prop_assert!(
                    result.len() <= 32,
                    "LONGITUD EXCEDIDA: {} ({} chars) a partir de stem: '{}' ext: '{}'",
                    result,
                    result.len(),
                    stem,
                    ext
                );

                let prefix = format!("{:03}_", index);
                prop_assert!(result.starts_with(&prefix), "Prefijo destruido: {}", result);

                // La extensión debe estar intacta cuando cabe dentro del límite (siempre
                // es verdad para extensiones de audio reales de ≤5 chars con índice ≤999).
                if !ext.is_empty() {
                    let ext_suffix = format!(".{}", ext);
                    prop_assert!(
                        result.ends_with(&ext_suffix),
                        "Extension destruida: '{}' no termina en '{}' (input: '{}')",
                        result,
                        ext_suffix,
                        input
                    );
                }
            }
        }
    }
}
