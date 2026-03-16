# Contribución y Desarrollo

## Bienvenido! 👋

Legacy Audio Provisioner es un proyecto de **Spec-Driven Development** enfocado en la compatibilidad con sistemas heredados. Contribuciones que mantengan este espíritu son bienvenidas.

## Antes de Empezar

Familiarízate con:
1. **SPECIFICATION**: Leer [docs/spec/spec_driven_development.md](docs/spec/spec_driven_development.md)
2. **ARCHITECTURE**: Entender el flujo en [docs/architecture/architecture_overview.md](docs/architecture/architecture_overview.md)
3. **CODE**: Revisar la estructura actual en `src/`

## Setup de Desarrollo

```bash
# Clonar el repositorio
git clone https://github.com/yourusername/legacy-audio-provisioner
cd legacy-audio-provisioner

# Instalar Rust (si no está)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Compilar y probar
make build
make test
```

## Herramientas Útiles

### Recomendadas

```bash
# Watch files y auto-rebuild
cargo install cargo-watch
make dev

# Linting automático
cargo install clippy

# Coverage de tests
cargo install cargo-tarpaulin
make coverage

# Formateador de código
rustup component add rustfmt
```

### Documentación

```bash
# Generary y ver documentación
make docs

# Ver módulos internos
cargo doc --open
```

## Convenciones de Código

### Estilo

```rust
// ✓ Correcto: nombres claros, comentarios en español
/// R-03: Sanitización de Nombres
pub fn sanitize_filename(input: &str) -> String {
    input.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .take(32)
        .collect()
}

// ✗ Evitar: nombres genéricos, sin documentación
pub fn clean(s: &str) -> String {
    // ...
}
```

### Documentación

```rust
/// Descripción clara en una línea
///
/// Párrafo adicional si necesario:
/// - Punto 1
/// - Punto 2
///
/// # Arguments
/// * `param1` - Descripción
///
/// # Returns
/// Descripción del retorno
///
/// # Example
/// ```
/// let result = my_function("input");
/// assert_eq!(result, "expected");
/// ```
pub fn my_function(param1: &str) -> Result<String> {
    // ...
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_case() {
        let input = "normal_input.mp3";
        let result = sanitize_filename(input);
        assert_eq!(result, "normal_input.mp3");
    }

    #[test]
    fn test_edge_case_with_wide_characters() {
        let input = "canción_españa_🎵.mp3";
        let result = sanitize_filename(input);
        assert!(!result.contains("ó"));
        assert!(!result.contains("🎵"));
    }

    #[test]
    fn test_enforces_length_limit() {
        let long_name = "a".repeat(50);
        let result = sanitize_filename(&long_name);
        assert!(result.len() <= 32);
    }
}
```

## Workflow de Desarrollo

### 1. Crear una rama

```bash
# Feature nueva
git checkout -b feature/implement-id3-stripper

# Bugfix
git checkout -b bugfix/fix-emoji-handling

# Improvement
git checkout -b improve/reduce-memory-usage
```

### 2. Desarrollo iterativo

```bash
# Usar make
make dev    # Watch + auto-rebuild

# O manual
cargo build
cargo test
cargo clippy
```

### 3. Formato y Lint

```bash
# Antes de commit
make fmt    # Auto-format
make lint   # Check warnings
make test   # Asegurar tests pasan
```

### 4. Commit y Push

```bash
git add .
git commit -m "feat(sanitizer): support unicode normalization"
git push origin feature/implement-unicode-support
```

### 5. Pull Request

Describe:
- **Qué** cambió
- **Por qué** es necesario
- **Cómo** cumple la especificación
- Tests agregados/modificados

## Tareas por Prioridad

### High Priority (Bloquean release 0.1)

- [ ] Hardening de normalización de audio (perfil bitrate/codec por firmware)
- [ ] Política fina de metadatos (qué tags permitir/descartar)
- [ ] Verificación extendida post-provisioning
- [ ] Cobertura E2E destructiva adicional en CI

### Medium Priority (Para 0.2)

- [ ] Normalización de bitrate (VBR→CBR)
- [ ] Interfaz interactiva con confirmaciones
- [ ] Soporte multi-plataforma (Windows, macOS)
- [ ] Progress bars (`indicatif`)

### Low Priority (Enhancements)

- [ ] Config file support (TOML/YAML)
- [ ] Logging a archivo
- [ ] Estadísticas y reportes más detallados
- [ ] Soporte para FLAC, WAV normalization
- [ ] GUI (GTK/Qt)

## Áreas Específicas de Desarrollo

### 1. Audio Processing (`normalizer.rs`)

**Estado**: Pipeline activo con `ffprobe` + `ffmpeg`

**TODO**:
```rust
// Afinar política de metadatos por perfil de hardware legacy
pub fn strip_id3v2(file_path: &Path) -> Result<()> {
    // Estado actual: limpieza base vía ffmpeg (-map_metadata -1)
    // TODO: permitir lista blanca de tags críticos si aplica
    //
    // 1. Leer archivo
    // 2. Ubicar ID3v2 header (3 bytes: "ID3")
    // 3. Calcular tamaño (bytes 6-9 con synchsafe encoding)
    // 4. Crear nuevo archivo sin los primeros N bytes
    // 5. Reemplazar original
}

// Hardening de validación bitrate/codec
pub fn normalize_bitrate(file_path: &Path, target_kbps: u32) -> Result<()> {
    // Estado actual: transcodificación a MP3 CBR para entradas incompatibles
    // TODO: agregar perfiles por generación de firmware
}
```

### 2. Device Detection (`hardware.rs`)

**Estado**: Implementado (`/proc/mounts` + `/sys/block/*/removable` + `statvfs`)

**TODO**:
```rust
// Auto-detect USB devices from /proc/mounts
pub fn detect_usb_devices() -> Result<Vec<DeviceInfo>> {
    // Parsear /proc/mounts:
    // /dev/sdb1 /media/user/DISK vfat defaults 0 0
    //
    // Verificar /sys/block/sdb/removable
    // Leer espacio disponible con statvfs
}
```

### 3. Backup & Copy (`backup.rs`)

**Estado**: Copia y checksum en producción (`std::fs`, SHA256 streaming)

**TODO**:
```rust
// Ejemplo de copia nativa y verificación (actual)
pub fn copy_directory_with_progress(
    source: &Path,
    dest: &Path,
) -> Result<u64> {
    // Copia nativa usando std::fs + hashing SHA256 en streaming
    // (ver implementación real en src/backup.rs)
}
```

### 4. Verification (`verification.rs`)

**Estado**: Reportes

**TODO**:
```rust
// Recorrer USB y verificar estructura
pub fn verify_directory_structure(usb_path: &Path) -> Result<VerificationReport> {
    use walkdir::WalkDir;

    let mut report = VerificationReport::new();

    // Check 1: Solo 2 niveles de profundidad
    for entry in WalkDir::new(usb_path)
        .max_depth(3)
    {
        // Si depth > 2: error
    }

    // Check 2: Max 50 archivos por carpeta
    let mut folder_counts = HashMap::new();
    for entry in WalkDir::new(usb_path) {
        // Contar archivos por carpeta
    }

    // Check 3: Nombres sanitizados
    for entry in WalkDir::new(usb_path) {
        // Verificar nombre cumple regex
    }

    // Check 4: Checksums
    for file in collected_files {
        let actual = compute_sha256(file)?;
        if actual != expected_checksum {
            report.add_error(...);
        }
    }
}
```

## Debugging

### Logs Detallados

```bash
# Diferentes niveles
RUST_LOG=trace cargo run -- --verbose
RUST_LOG=debug cargo run -- -vv
RUST_LOG=info cargo run --
```

### Print Debugging

```rust
// En tests
println!("Debug info: {:?}", variable);

// En código
eprintln!("Error info: {}", error);

// Mejor: usar log! macro
log::debug!("Processing file: {}", path.display());
```

### Usando Debugger (GDB/LLDB)

```bash
# Compilar con symbols
cargo build --verbose

# Con GDB (Linux)
gdb ./target/debug/legacy-audio-provisioner
(gdb) run --usb-mount /tmp/test --audio-source .
(gdb) break filename.rs:42
(gdb) continue
(gdb) print variable

# Con LLDB (macOS)
lldb ./target/debug/legacy-audio-provisioner
(lldb) breakpoint set --file sanitizer.rs --line 42
```

## Performance Profiling

```bash
# Compilar en release
cargo build --release

# Usar perf (Linux)
perf record -g ./target/release/legacy-audio-provisioner ...
perf report

# O usar flamegraph
cargo install flamegraph
cargo flamegraph -- --usb-mount /tmp --audio-source .
```

## Documentación de Cambios

### Actualizar especificación

Si cambias el comportamiento, actualiza:

1. `docs/spec/spec_driven_development.md` (seccion relevante)
2. `README.md` (features list)
3. Docstrings en código (comentarios R-XX)

### Changelog

Mantener `CHANGELOG.md` (si se crea):

```
## [0.2.0] - 2026-03-20
### Added
- Auto device detection from /proc/mounts
- ID3v2 tag stripping

### Fixed
- Checksum validation timing issue

### Changed
- Backup directory location format
```

## Code Review Checklist

Antes de hacer push:

- [ ] Tests pasan: `make test`
- [ ] Código formateado: `make fmt`
- [ ] No hay warnings: `make lint`
- [ ] Documentación clara (docstrings)
- [ ] Ejemplos en tests (cuando aplica)
- [ ] Cambios documentados (`README.md` y `docs/architecture/architecture_overview.md`)
- [ ] Commits con mensajes claros

## Reportar Issues

Usar GitHub Issues con template:

```markdown
### Description
Qué está pasando mal / qué feature falta

### Reproduce
Pasos para reproducir:
1. Compilar: `cargo build`
2. Ejecutar: `./target/debug/... --usb-mount /tmp`
3. Observar error

### Expected
Qué debería pasar

### Environment
- OS: Linux 6.2 / macOS 14 / Windows 11
- Rust: `rustc --version`
- USB: Size, filesystem, removable: yes/no
```

## Preguntas?

- Leer `docs/architecture/architecture_overview.md` 📖
- Revisar tests en `src/**/*.rs` 🧪
- Abrir issue para discusión 💬

**Happy coding! 🚀**
