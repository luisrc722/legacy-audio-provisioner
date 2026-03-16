//! R-07: Distribucion de Carga (Redistribucion por Segmentos)
//!
//! Requisitos:
//! - Limite de 50 archivos por directorio
//! - Crear carpetas VOL_01, VOL_02, etc.
//! - NOTA: Este modulo es exclusivamente de planificacion en memoria.
//!   La ejecucion fisica (I/O) se delega al orquestador para integrar FFmpeg y Checkpoints.

use anyhow::Result;
use log::info;
use std::path::PathBuf;

pub const MAX_FILES_PER_FOLDER: usize = 50;

#[derive(Debug, Clone)]
pub struct DistributedFile {
    pub source_path: PathBuf,
    pub sanitized_name: String,
}

#[derive(Debug, Clone)]
pub struct VolumeSegment {
    pub folder_name: String,
    pub volume_index: usize,
    pub files: Vec<DistributedFile>,
}

impl VolumeSegment {
    pub fn new(volume_index: usize) -> Self {
        VolumeSegment {
            folder_name: format!("VOL_{:02}", volume_index),
            volume_index,
            files: Vec::new(),
        }
    }

    pub fn add_file(&mut self, file: DistributedFile) {
        self.files.push(file);
    }

    pub fn is_full(&self) -> bool {
        self.files.len() >= MAX_FILES_PER_FOLDER
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Planifica la distribucion en memoria.
/// Esta funcion es pura: solo calcula la topologia.
pub fn plan_distribution(file_mappings: Vec<(PathBuf, String)>) -> Result<Vec<VolumeSegment>> {
    let total_files = file_mappings.len();
    if total_files == 0 {
        return Ok(Vec::new());
    }

    let num_volumes = (total_files.saturating_add(MAX_FILES_PER_FOLDER - 1)) / MAX_FILES_PER_FOLDER;
    info!(
        "Planning distribution of {} files into {} volume(s)",
        total_files, num_volumes
    );

    let mut volumes = Vec::new();

    for (idx, chunk) in file_mappings.chunks(MAX_FILES_PER_FOLDER).enumerate() {
        let mut volume = VolumeSegment::new(idx + 1);

        for (source_path, sanitized_name) in chunk {
            volume.add_file(DistributedFile {
                source_path: source_path.clone(),
                sanitized_name: sanitized_name.clone(),
            });
        }

        info!(
            "Volume {}: {} files planned",
            volume.volume_index,
            volume.file_count()
        );
        volumes.push(volume);
    }

    Ok(volumes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_segment_creation() {
        let volume = VolumeSegment::new(1);
        assert_eq!(volume.folder_name, "VOL_01");
        assert_eq!(volume.volume_index, 1);
        assert_eq!(volume.file_count(), 0);
        assert!(!volume.is_full());
    }

    #[test]
    fn test_volume_capacity() {
        let mut volume = VolumeSegment::new(1);
        for i in 0..50 {
            volume.add_file(DistributedFile {
                source_path: PathBuf::from(format!("/fake/path_{}.mp3", i)),
                sanitized_name: format!("file_{:03}.mp3", i),
            });
        }
        assert_eq!(volume.file_count(), 50);
        assert!(volume.is_full());
    }

    #[test]
    fn test_planning_single_volume() -> Result<()> {
        let mut mappings = Vec::new();
        for i in 0..30 {
            mappings.push((
                PathBuf::from(format!("/fake/song_{:03}.mp3", i)),
                format!("{:03}_song.mp3", i),
            ));
        }

        let volumes = plan_distribution(mappings)?;
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].file_count(), 30);
        Ok(())
    }

    #[test]
    fn test_planning_multiple_volumes() -> Result<()> {
        let mut mappings = Vec::new();
        for i in 0..125 {
            mappings.push((
                PathBuf::from(format!("/fake/song_{:03}.mp3", i)),
                format!("{:03}_song.mp3", i),
            ));
        }

        let volumes = plan_distribution(mappings)?;
        assert_eq!(volumes.len(), 3);
        assert_eq!(volumes[0].file_count(), 50);
        assert_eq!(volumes[1].file_count(), 50);
        assert_eq!(volumes[2].file_count(), 25);
        Ok(())
    }
}
