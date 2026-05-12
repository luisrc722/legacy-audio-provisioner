//! Audio Discovery Module
//!
//! Busca y cataloga archivos de audio dentro de una USB o directorio.
//! Ignora proactivamente archivos y directorios ocultos (ej. .DS_Store, ._archivos)
//! para evitar crasheos en firmwares legacy.
//!
//! Soporta formatos: MP3, FLAC, WAV, OGG, M4A, ALAC, AAC, WMA, OPUS, AIFF
//!
//! Requisitos:
//! - Escaneo recursivo del dispositivo
//! - Filtrado de archivos ocultos (AppleDouble, dotfiles)
//! - Identificación de archivos de audio por extensión
//! - Reporte del total de archivos y espacio ocupado
//! - Soporte para límite de profundidad configurable

use anyhow::{anyhow, Result};
use log::info;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

/// Formatos de audio soportados
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "m4a", "alac", "aac", "wma", "opus", "aiff",
];

/// Información sobre un archivo de audio encontrado
#[derive(Debug, Clone)]
pub struct AudioFile {
    /// Ruta completa del archivo
    pub path: PathBuf,

    /// Nombre del archivo
    pub filename: String,

    /// Extensión del archivo (sin punto)
    pub extension: String,

    /// Tamaño en bytes
    pub size_bytes: u64,

    /// Profundidad de directorio (0 = raíz)
    pub depth: usize,
}

impl AudioFile {
    /// Obtener tamaño en MB
    pub fn size_mb(&self) -> f64 {
        self.size_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Obtener tamaño en GB
    pub fn size_gb(&self) -> f64 {
        self.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

/// Reporte de búsqueda de audio
#[derive(Debug, Clone)]
pub struct AudioDiscoveryReport {
    /// Directorio raíz escaneado
    pub root_path: PathBuf,

    /// Total de archivos de audio encontrados
    pub total_files: usize,

    /// Archivos de audio encontrados
    pub audio_files: Vec<AudioFile>,

    /// Tamaño total en bytes
    pub total_size_bytes: u64,

    /// Profundidad máxima encontrada
    pub max_depth: usize,

    /// Directorios escaneados
    pub directories_scanned: usize,

    /// Error durante escaneo (si aplica)
    pub scan_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AudioDiscoveryStats {
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub max_depth: usize,
    pub directories_scanned: usize,
}

impl AudioDiscoveryReport {
    /// Obtener tamaño total en MB
    pub fn total_size_mb(&self) -> f64 {
        self.total_size_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Obtener tamaño total en GB
    pub fn total_size_gb(&self) -> f64 {
        self.total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Obtener tamaño promedio por archivo
    pub fn average_file_size_mb(&self) -> f64 {
        if self.total_files == 0 {
            0.0
        } else {
            self.total_size_mb() / self.total_files as f64
        }
    }

    /// Agrupar archivos por extensión
    pub fn group_by_extension(&self) -> std::collections::HashMap<String, Vec<&AudioFile>> {
        let mut groups = std::collections::HashMap::new();
        for file in &self.audio_files {
            groups
                .entry(file.extension.clone())
                .or_insert_with(Vec::new)
                .push(file);
        }
        groups
    }

    /// Verificar si el volumen está vacío (sin música)
    pub fn is_empty(&self) -> bool {
        self.total_files == 0
    }
}

/// Helper para determinar si una entrada es basura de sistema.
/// Bloquea dotfiles (macOS/Linux) y directorios ocultos nativos de Windows.
/// CRITICO PARA LOS TESTS: el directorio raiz (depth 0) siempre se permite.
fn is_hidden(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }

    let file_name = entry.file_name().to_string_lossy();

    file_name.starts_with('.')
        || file_name == "System Volume Information"
        || file_name == "$RECYCLE.BIN"
        || file_name.starts_with("FOUND.")
}

/// Buscar archivos de audio en un directorio
/// Ignora automáticamente archivos y carpetas ocultas (.DS_Store, ._files, .Trash, etc.)
pub fn discover_audio_files(root_path: &Path) -> Result<AudioDiscoveryReport> {
    discover_audio_core(root_path, usize::MAX)
}

pub fn count_audio_files(root_path: &Path) -> Result<usize> {
    Ok(visit_audio_files(root_path, |_| Ok(()))?.total_files)
}

pub fn visit_audio_files<F>(root_path: &Path, on_file: F) -> Result<AudioDiscoveryStats>
where
    F: FnMut(AudioFile) -> Result<()>,
{
    visit_audio_files_limited_depth(root_path, usize::MAX, on_file)
}

/// Buscar archivos de audio con límite de profundidad
/// Ignora automáticamente archivos y carpetas ocultas
pub fn discover_audio_files_limited_depth(
    root_path: &Path,
    max_depth: usize,
) -> Result<AudioDiscoveryReport> {
    // +1 porque WalkDir cuenta la raíz como profundidad 0
    discover_audio_core(root_path, max_depth.saturating_add(1))
}

pub fn visit_audio_files_limited_depth<F>(
    root_path: &Path,
    max_depth: usize,
    on_file: F,
) -> Result<AudioDiscoveryStats>
where
    F: FnMut(AudioFile) -> Result<()>,
{
    visit_audio_core(root_path, max_depth.saturating_add(1), on_file)
}

/// Motor principal de búsqueda (implementación D.R.Y. - Don't Repeat Yourself)
/// Refactorización que elimina código duplicado entre las dos funciones públicas
fn discover_audio_core(root_path: &Path, max_depth: usize) -> Result<AudioDiscoveryReport> {
    let mut audio_files = Vec::new();
    let stats = visit_audio_core(root_path, max_depth, |audio_file| {
        audio_files.push(audio_file);
        Ok(())
    })?;

    // Ordenar archivos por ruta para reproducibilidad
    audio_files.sort_by(|a, b| a.path.cmp(&b.path));

    let total_files = audio_files.len();
    info!(
        "Audio discovery complete: {} files, {:.2} GB (directories: {})",
        total_files,
        stats.total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        stats.directories_scanned
    );

    Ok(AudioDiscoveryReport {
        root_path: root_path.to_path_buf(),
        total_files,
        audio_files,
        total_size_bytes: stats.total_size_bytes,
        max_depth: stats.max_depth,
        directories_scanned: stats.directories_scanned,
        scan_error: None,
    })
}

fn visit_audio_core<F>(root_path: &Path, max_depth: usize, mut on_file: F) -> Result<AudioDiscoveryStats>
where
    F: FnMut(AudioFile) -> Result<()>,
{
    if !root_path.exists() {
        return Err(anyhow!("Path does not exist: {}", root_path.display()));
    }

    if !root_path.is_dir() {
        return Err(anyhow!("Path is not a directory: {}", root_path.display()));
    }

    info!(
        "Starting secure audio discovery in: {} (max_depth={})",
        root_path.display(),
        max_depth
    );

    let mut stats = AudioDiscoveryStats::default();

    let walker = WalkDir::new(root_path)
        .max_depth(max_depth)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok());

    for entry in walker {
        let path = entry.path();
        let depth = entry.depth();

        if depth > stats.max_depth {
            stats.max_depth = depth;
        }

        if path.is_dir() {
            stats.directories_scanned += 1;
            continue;
        }

        if let Some(ext) = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
        {
            if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                if let Ok(metadata) = entry.metadata() {
                    let size = metadata.len();
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    let audio_file = AudioFile {
                        path: path.to_path_buf(),
                        filename,
                        extension: ext,
                        size_bytes: size,
                        depth: depth.saturating_sub(1),
                    };

                    stats.total_files += 1;
                    stats.total_size_bytes += size;

                    info!(
                        "Found audio file: {} ({:.2} MB)",
                        path.display(),
                        size as f64 / (1024.0 * 1024.0)
                    );
                    on_file(audio_file)?;
                }
            }
        }
    }

    info!(
        "Audio discovery complete: {} files, {:.2} GB (directories: {})",
        stats.total_files,
        stats.total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        stats.directories_scanned
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_ignores_hidden_files() -> Result<()> {
        let temp_dir = TempDir::new()?;

        File::create(temp_dir.path().join("song1.mp3"))?;
        File::create(temp_dir.path().join("._song2.mp3"))?; // Archivo oculto tipo macOS (AppleDouble)

        let report = discover_audio_files(temp_dir.path())?;

        assert_eq!(report.total_files, 1);
        assert_eq!(report.audio_files[0].filename, "song1.mp3");
        info!("✓ Hidden files test passed");

        Ok(())
    }

    #[test]
    fn test_ignores_hidden_directories() -> Result<()> {
        let temp_dir = TempDir::new()?;

        fs::create_dir(temp_dir.path().join(".Trash"))?;
        File::create(temp_dir.path().join(".Trash/deleted_song.mp3"))?;
        File::create(temp_dir.path().join("good_song.mp3"))?;

        let report = discover_audio_files(temp_dir.path())?;

        assert_eq!(report.total_files, 1);
        assert_eq!(report.audio_files[0].filename, "good_song.mp3");
        info!("✓ Hidden directories test passed");

        Ok(())
    }

    #[test]
    fn test_discover_audio_files_empty_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let report = discover_audio_files(temp_dir.path())?;

        assert_eq!(report.total_files, 0);
        assert_eq!(report.total_size_bytes, 0);
        assert!(report.is_empty());

        Ok(())
    }

    #[test]
    fn test_discover_audio_files_with_mp3() -> Result<()> {
        let temp_dir = TempDir::new()?;

        File::create(temp_dir.path().join("song1.mp3"))?;
        File::create(temp_dir.path().join("song2.mp3"))?;
        File::create(temp_dir.path().join("readme.txt"))?; // No debería detectarse

        let report = discover_audio_files(temp_dir.path())?;

        assert_eq!(report.total_files, 2);
        assert_eq!(report.audio_files.len(), 2);
        assert!(!report.is_empty());

        for file in &report.audio_files {
            assert_eq!(file.extension, "mp3");
        }

        Ok(())
    }

    #[test]
    fn test_discover_multiple_audio_formats() -> Result<()> {
        let temp_dir = TempDir::new()?;

        File::create(temp_dir.path().join("song.mp3"))?;
        File::create(temp_dir.path().join("audio.flac"))?;
        File::create(temp_dir.path().join("track.wav"))?;
        File::create(temp_dir.path().join("music.ogg"))?;

        let report = discover_audio_files(temp_dir.path())?;

        assert_eq!(report.total_files, 4);
        let group_by_ext = report.group_by_extension();
        assert_eq!(group_by_ext.len(), 4); // 4 extensiones diferentes

        Ok(())
    }

    #[test]
    fn test_discover_nested_directories() -> Result<()> {
        let temp_dir = TempDir::new()?;

        fs::create_dir(temp_dir.path().join("music"))?;
        fs::create_dir(temp_dir.path().join("music/rock"))?;

        File::create(temp_dir.path().join("song1.mp3"))?;
        File::create(temp_dir.path().join("music/song2.mp3"))?;
        File::create(temp_dir.path().join("music/rock/song3.flac"))?;

        let report = discover_audio_files(temp_dir.path())?;

        assert_eq!(report.total_files, 3);
        assert!(report.max_depth > 0);
        assert!(report.directories_scanned > 1);

        Ok(())
    }

    #[test]
    fn test_audio_file_size_calculations() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let file_path = temp_dir.path().join("large_song.mp3");
        let mut file = File::create(&file_path)?;

        // Escribir 1 MB de datos
        use std::io::Write;
        file.write_all(&vec![0u8; 1024 * 1024])?;

        let report = discover_audio_files(temp_dir.path())?;

        assert_eq!(report.total_files, 1);
        assert_eq!(report.audio_files[0].size_mb() as u32, 1);
        assert!(report.total_size_mb() >= 1.0);

        Ok(())
    }

    #[test]
    fn test_nonexistent_directory_fails() {
        let result = discover_audio_files(Path::new("/nonexistent/path/12345"));
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_with_depth_limit() -> Result<()> {
        let temp_dir = TempDir::new()?;

        fs::create_dir_all(temp_dir.path().join("a/b/c/d"))?;

        File::create(temp_dir.path().join("song1.mp3"))?;
        File::create(temp_dir.path().join("a/song2.mp3"))?;
        File::create(temp_dir.path().join("a/b/song3.mp3"))?;
        File::create(temp_dir.path().join("a/b/c/song4.mp3"))?;
        File::create(temp_dir.path().join("a/b/c/d/song5.mp3"))?;

        // Con límite de profundidad 2, debería encontrar solo song1, song2, song3
        let report = discover_audio_files_limited_depth(temp_dir.path(), 2)?;

        assert_eq!(report.total_files, 3);

        Ok(())
    }

    #[test]
    fn test_visit_audio_files_streams_entries() -> Result<()> {
        let temp_dir = TempDir::new()?;

        File::create(temp_dir.path().join("a.mp3"))?;
        File::create(temp_dir.path().join("b.flac"))?;
        File::create(temp_dir.path().join("c.txt"))?;

        let mut visited = Vec::new();
        let stats = visit_audio_files(temp_dir.path(), |audio_file| {
            visited.push(audio_file.filename);
            Ok(())
        })?;

        assert_eq!(stats.total_files, 2);
        assert_eq!(visited, vec!["a.mp3".to_string(), "b.flac".to_string()]);
        Ok(())
    }
}
