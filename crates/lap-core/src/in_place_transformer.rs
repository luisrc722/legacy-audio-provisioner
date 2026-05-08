use crate::audio_discovery;
use crate::distribution;
use crate::sanitizer;
use crate::security::validate_path_containment;
use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InPlacePlanEntry {
    pub index: usize,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub volume_name: String,
    pub normalized_name: String,
}

#[derive(Debug, Clone)]
pub struct InPlacePlan {
    pub entries: Vec<InPlacePlanEntry>,
}

pub struct InPlaceTransformer;

impl InPlaceTransformer {
    pub fn build_plan(usb_mount: &Path) -> Result<InPlacePlan> {
        let discovery_report = audio_discovery::discover_audio_files(usb_mount)?;

        let file_mappings: Vec<(PathBuf, String)> = discovery_report
            .audio_files
            .iter()
            .enumerate()
            .map(|(idx, file)| {
                let sanitized = sanitizer::sanitize_filename(&file.filename);
                let indexed = sanitizer::add_sequential_prefix(&sanitized, idx + 1);
                (file.path.clone(), indexed)
            })
            .collect();

        let volumes = distribution::plan_distribution(file_mappings)?;

        let mut entries = Vec::new();
        let mut global_index = 0usize;

        for volume in volumes {
            let volume_dir = validate_path_containment(usb_mount, Path::new(&volume.folder_name))?;
            for file in volume.files {
                let destination_path =
                    validate_path_containment(&volume_dir, Path::new(&file.sanitized_name))?;

                entries.push(InPlacePlanEntry {
                    index: global_index,
                    source_path: file.source_path,
                    destination_path,
                    volume_name: volume.folder_name.clone(),
                    normalized_name: file.sanitized_name,
                });

                global_index += 1;
            }
        }

        Ok(InPlacePlan { entries })
    }
}