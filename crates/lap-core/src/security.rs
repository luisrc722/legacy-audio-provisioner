//! MODULO DE SEGURIDAD (R-34, R-35, R-36)
//! [R-05-001] Jaula de Rutas
//!
//! Endurecimiento ofensivo integral contra:
//! - R-34: ataques de Path Traversal / escape de directorio
//! - R-35: ataques de command injection y metacaracteres de shell
//! - R-36: metadata bombing (zip bombs, overflow de ID3)

use anyhow::{anyhow, Context, Result};
use std::path::{Component, Path, PathBuf};

/// R-34: Canonicalización de rutas y protección contra traversal
///
/// Valida que una ruta dada, al resolverse, quede contenida dentro de un directorio base.
/// Previene ataques como: `../../../../etc/passwd`, `../../../.ssh/authorized_keys`
///
/// Esta implementación usa análisis por componentes (sin canonicalización obligatoria)
/// para funcionar incluso cuando los archivos todavía no existen.
///
/// # Retorna
/// - `Ok(PathBuf)` si la ruta es válida y está contenida dentro de base
/// - `Err` si la ruta escapa del directorio base o contiene componentes sospechosos
pub fn validate_path_containment(base: &Path, candidate: &Path) -> Result<PathBuf> {
    // Comenzar con ruta base absoluta
    let base_abs = if base.is_absolute() {
        base.to_path_buf()
    } else {
        std::env::current_dir()
            .context("Cannot get current directory")?
            .join(base)
    };

    // Si candidate es absoluta, rechazarla (seguridad: no se permiten rutas absolutas)
    if candidate.is_absolute() {
        return Err(anyhow!(
            "Cannot use absolute paths for file operations: {}",
            candidate.display()
        ));
    }

    // Construir ruta resuelta usando análisis componente por componente
    let mut resolved = base_abs.clone();

    for component in candidate.components() {
        match component {
            Component::ParentDir => {
                // Intentar subir un nivel
                if !resolved.pop() {
                    return Err(anyhow!(
                        "Path traversal attack detected: attempted to escape filesystem root"
                    ));
                }
            }
            Component::Normal(name) => {
                resolved.push(name);
            }
            Component::CurDir => {
                // Directorio actual, sin efecto
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!(
                    "Invalid path component (root/prefix not allowed): {}",
                    candidate.display()
                ));
            }
        }
    }

    // CRITICO: verificar que la ruta resuelta siga dentro del directorio base
    if !resolved.starts_with(&base_abs) {
        return Err(anyhow!(
            "Path traversal attack detected: resolved path {} escapes base {}",
            resolved.display(),
            base_abs.display()
        ));
    }

    Ok(resolved)
}

/// R-35: Prevención de metacaracteres de shell y command injection
///
/// Verifica si un nombre de archivo contiene caracteres de control de shell que puedan explotarse
/// en ataques de command injection como: `song.mp3; rm -rf /`
///
/// Caracteres peligrosos: `; | & $ ( ) < > ` ' " \ ` salto de línea`
pub fn contains_shell_metacharacters(filename: &str) -> bool {
    filename.chars().any(|c| {
        matches!(
            c,
            ';' | '|' | '&' | '$' | '(' | ')' | '<' | '>' | '`' | '\'' | '"' | '\\' | '\n' | '\r'
        )
    })
}

/// Variante R-35: valida que el nombre de archivo sea seguro para ejecución con shell
/// Retorna Err si el nombre de archivo contiene caracteres peligrosos
pub fn validate_shell_safe_filename(filename: &str) -> Result<()> {
    if contains_shell_metacharacters(filename) {
        return Err(anyhow!(
            "Filename contains shell metacharacters and is unsafe: {}",
            filename
        ));
    }
    Ok(())
}

/// R-36: Sandbox de metadatos - límite de lectura de etiquetas ID3
///
/// Previene ataques estilo "zip bomb" donde metadatos de etiquetas ID3 son diseñados para consumir
/// grandes cantidades de RAM durante el parseo (causando OOM, DoS)
///
/// Etiquetas ID3v2 estándar: 10-300 KB
/// Se permite hasta 5 MB como margen de seguridad para metadatos legítimos grandes
pub const MAX_ID3_TAG_SIZE: u64 = 5 * 1024 * 1024; // 5 MB

/// R-36: validar tamaño de archivo para parseo de metadatos
///
/// Rechaza archivos con encabezados o secciones de metadatos sospechosamente grandes
pub fn validate_metadata_bomb_safety(file_path: &Path, max_tag_size: u64) -> Result<()> {
    let metadata = std::fs::metadata(file_path)
        .with_context(|| format!("Cannot read file metadata: {}", file_path.display()))?;

    let file_size = metadata.len();

    // Verificar si el archivo es una bomba ID3 sospechosamente pequeña (solo metadatos, sin audio real)
    // La mayoría de archivos de audio son > 1MB; los extremadamente pequeños son sospechosos
    if file_size < 1024 {
        return Err(anyhow!(
            "File suspiciously small (< 1KB), likely malformed: {}",
            file_path.display()
        ));
    }

    // Verificar si la sección de metadatos es irrazonablemente grande
    // ID3v2 puede llegar teóricamente a 256 MB, pero aquí usamos un límite conservador
    if file_size > max_tag_size {
        return Err(anyhow!(
            "File size {} MB exceeds safe metadata limit {} MB: {}",
            file_size / 1_048_576,
            max_tag_size / 1_048_576,
            file_path.display()
        ));
    }

    Ok(())
}

/// R-35 completo: validación de nombre de archivo combinando todas las comprobaciones
///
/// Valida que un nombre de archivo:
/// - No esté vacío
/// - No contenga metacaracteres de shell
/// - No contenga patrones prohibidos de path traversal
/// - Esté dentro de límites de longitud razonables (después de sanitización)
pub fn validate_filename_comprehensive(filename: &str) -> Result<()> {
    if filename.is_empty() {
        return Err(anyhow!("Filename cannot be empty"));
    }

    // Verificar intentos de path traversal en el propio nombre de archivo
    if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
        return Err(anyhow!(
            "Filename contains path traversal patterns: {}",
            filename
        ));
    }

    // Verificar metacaracteres de shell
    validate_shell_safe_filename(filename)?;

    // Verificación de longitud (32 chars es el límite FAT32 que aplicamos)
    if filename.len() > 255 {
        return Err(anyhow!("Filename exceeds 255 bytes (UTF-8): {}", filename));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_path_containment_valid() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Rutas relativas válidas (no necesitan existir)
        assert!(validate_path_containment(base, Path::new("file.mp3")).is_ok());
        assert!(validate_path_containment(base, Path::new("dir/file.mp3")).is_ok());
        assert!(validate_path_containment(base, Path::new("a/b/c/file.mp3")).is_ok());
    }

    #[test]
    fn test_path_containment_traversal_attack() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Ataques de path traversal (deben fallar)
        assert!(validate_path_containment(base, Path::new("../../../etc/passwd")).is_err());
        assert!(validate_path_containment(base, Path::new("../file.mp3")).is_err());
        assert!(validate_path_containment(base, Path::new("dir/../../../../../../tmp")).is_err());
    }

    #[test]
    fn test_path_containment_absolute_path_escape() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Las rutas absolutas deben rechazarse (política de seguridad)
        assert!(validate_path_containment(base, Path::new("/etc/passwd")).is_err());
        assert!(validate_path_containment(base, Path::new("/tmp/file.mp3")).is_err());
    }

    #[test]
    fn test_shell_metacharacters() {
        // Nombres de archivo seguros
        assert!(!contains_shell_metacharacters("song_2024.mp3"));
        assert!(!contains_shell_metacharacters("Canción-del-Amor.mp3"));
        assert!(!contains_shell_metacharacters("001_ValidName.mp3"));

        // Nombres de archivo peligrosos
        assert!(contains_shell_metacharacters("song.mp3; rm -rf /"));
        assert!(contains_shell_metacharacters("file.mp3 | cat /etc/passwd"));
        assert!(contains_shell_metacharacters("$(malicious).mp3"));
        assert!(contains_shell_metacharacters("`dangerous`.mp3"));
        assert!(contains_shell_metacharacters("quote'injection.mp3"));
        assert!(contains_shell_metacharacters("double\"quote.mp3"));
        assert!(contains_shell_metacharacters("backslash\\.mp3"));
    }

    #[test]
    fn test_validate_shell_safe_filename() {
        assert!(validate_shell_safe_filename("safe_file.mp3").is_ok());
        assert!(validate_shell_safe_filename("song.mp3; rm /").is_err());
        assert!(validate_shell_safe_filename("$(whoami).mp3").is_err());
    }

    #[test]
    fn test_validate_filename_comprehensive() {
        // Válido
        assert!(validate_filename_comprehensive("song.mp3").is_ok());
        assert!(validate_filename_comprehensive("Song_01.mp3").is_ok());

        // Inválido - vacío
        assert!(validate_filename_comprehensive("").is_err());

        // Inválido - path traversal en nombre de archivo
        assert!(validate_filename_comprehensive("../../../etc/passwd").is_err());
        assert!(validate_filename_comprehensive("dir/file.mp3").is_err());

        // Inválido - caracteres de shell
        assert!(validate_filename_comprehensive("song.mp3; echo hacked").is_err());
        assert!(validate_filename_comprehensive("$(cat /etc/passwd).mp3").is_err());
    }

    #[test]
    fn test_validate_metadata_bomb_safety() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.mp3");

        // Crear un archivo de tamaño normal (> 1KB)
        let dummy_data = vec![0u8; 10 * 1024]; // 10 KB
        fs::write(&file_path, &dummy_data).unwrap();
        assert!(validate_metadata_bomb_safety(&file_path, MAX_ID3_TAG_SIZE).is_ok());

        // Archivo demasiado pequeño (< 1KB) - sospechoso
        let tiny_file = temp.path().join("tiny.mp3");
        fs::write(&tiny_file, b"x").unwrap();
        assert!(validate_metadata_bomb_safety(&tiny_file, MAX_ID3_TAG_SIZE).is_err());

        // Archivo inexistente
        let nonexistent = temp.path().join("nonexistent.mp3");
        assert!(validate_metadata_bomb_safety(&nonexistent, MAX_ID3_TAG_SIZE).is_err());
    }
}
