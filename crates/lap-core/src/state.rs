use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::PathBuf;

pub const LAP_STATE_DIR_ENV: &str = "LAP_STATE_DIR";

#[derive(Debug, Clone)]
pub struct DeviceStatePaths {
    pub root_dir: PathBuf,
    pub backup_base_dir: PathBuf,
    pub checkpoint_dir: PathBuf,
    pub checkpoint_file: PathBuf,
    pub manifest_file: PathBuf,
    pub journal_file: PathBuf,
}

pub fn state_root_dir() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var(LAP_STATE_DIR_ENV) {
        let path = PathBuf::from(custom);
        fs::create_dir_all(&path)
            .with_context(|| format!("No se pudo crear LAP_STATE_DIR '{}'", path.display()))?;
        return Ok(path);
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map_err(|_| anyhow!("No se pudo resolver HOME para estado operativo"))?;

    let root = home.join(".lap");
    fs::create_dir_all(&root)
        .with_context(|| format!("No se pudo crear directorio de estado '{}'", root.display()))?;
    Ok(root)
}

pub fn paths_for_device(device_key: &str) -> Result<DeviceStatePaths> {
    let root_dir = state_root_dir()?;
    let slug = sanitize_key(device_key);

    let backup_base_dir = root_dir.join("backups");
    let checkpoints_root = root_dir.join("checkpoints");
    let manifests_root = root_dir.join("manifests");
    let journals_root = root_dir.join("journals");

    fs::create_dir_all(&backup_base_dir)?;
    fs::create_dir_all(&checkpoints_root)?;
    fs::create_dir_all(&manifests_root)?;
    fs::create_dir_all(&journals_root)?;

    let checkpoint_dir = checkpoints_root.join(&slug);
    fs::create_dir_all(&checkpoint_dir)?;

    Ok(DeviceStatePaths {
        root_dir,
        backup_base_dir,
        checkpoint_file: checkpoint_dir.join(".provisioning_checkpoint"),
        checkpoint_dir,
        manifest_file: manifests_root.join(format!("manifest_{}.json", slug)),
        journal_file: journals_root.join(format!("journal_{}.json", slug)),
    })
}

fn sanitize_key(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_sep = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_sep = false;
            continue;
        }

        if !last_sep {
            out.push('_');
            last_sep = true;
        }
    }

    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "device".to_string()
    } else {
        trimmed.to_string()
    }
}
