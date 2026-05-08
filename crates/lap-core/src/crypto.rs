use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// [R-06-001] Politica de Hashing de Contenido
/// Referencia legacy: R-23/R-26.
/// Precondicion: `path` apunta a un archivo legible.
/// Postcondicion: retorna digest SHA256 hex del contenido exacto del archivo.
/// Invariante: el hash debe ser determinista para el mismo contenido binario.
pub fn compute_file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("Failed to open file for hashing: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}
