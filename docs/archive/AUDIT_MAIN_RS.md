# Auditoría Técnica: main.rs - Análisis Crítico

**Fecha**: 12 de Marzo de 2026
**Versión del Código**: 0.1.0
**Estado**: 🔴 FALLOS CRÍTICOS IDENTIFICADOS

---

## Resumen Ejecutivo

El esqueleto CLI está **bien diseñado**, pero hay **3 defectos arquitectónicos críticos** que impiden que la herramienta funcione realmente:

| # | Criticidad | Problema | Impacto |
|---|-----------|----------|--------|
| 1 | 🔴 **CRÍTICA** | No copia archivos realmente | USB queda vacío tras "provisioning" |
| 2 | 🔴 **CRÍTICA** | No filtra archivos ocultos/basura | Estéreo intenta leer metadatos corruptos |
| 3 | 🟠 **ALTA** | Manejo de extensiones no resiliente | Riesgo de perder `.mp3` en santización |

---

## 🟢 Lo Positivo (Análisis Detallado)

### 1. **CLI Impecable** ✅
```rust
#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, value_name = "PATH")]
    usb_mount: Option<PathBuf>,
    // ...
    #[arg(long)]
    dry_run: bool,
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}
```

**Evaluación**:
- ✅ Uso correcto de `clap::Parser` con atributos derivados
- ✅ Flags bien documentados (help, long_about)
- ✅ Verbosidad en cascada (`-v`, `-vv`, `-vvv`)
- ✅ Patrón de "dry-run" para simular cambios
- ✅ Soporte para múltiples modos (list-devices, scan-usb, provisioning)

**Verdict**: Esta es una interfaz **profesional y resiliente**.

---

### 2. **Manejo de Errores Global** ✅
```rust
fn main() -> Result<()> {
    // ...
    info!("=== Legacy Audio Provisioner ===");
    // ...
    provision_usb(&usb, &audio_source, args.dry_run)?
}
```

**Evaluación**:
- ✅ Devuelve `anyhow::Result<()>` desde main() - el patrón Rust correcto
- ✅ El operador `?` propaga errores automáticamente
- ✅ `log::info!()` para auditoría de operaciones
- ✅ Si algo falla, el programa termina con mensaje limpio

**Verdict**: El flujo de errores es **idiomático y resiliente**.

---

### 3. **Separación de Responsabilidades** ✅

```rust
fn provision_usb(...) -> Result<()> {
    // Paso 1: Validar
    // Paso 2: Crear backup
    // Paso 3: Escanear
    // Paso 4: Sanitizar
    // Paso 5: Distribuir
    // Paso 6: Verificar
}
```

**Evaluación**:
- ✅ Cada paso está claramente separado
- ✅ Logging visible del progreso (println! + info!)
- ✅ Estructura compatible con checkpoint/recovery
- ✅ Orden lógico: validación → backup → transformación → distribución → verificación

**Verdict**: La **arquitectura es sólida** y extensible para recuperación de fallos.

---

## 🔴 Los Problemas Críticos

### ❌ **PROBLEMA #1: Falsa Promesa en provision_usb - NO Copia Archivos Realmente**

#### Ubicación
Línea ~141-160 en `src/main.rs`

#### Código Problemático
```rust
// Paso 4: Sanitizar nombres
let sanitized_files: Vec<String> = audio_files
    .iter()
    .enumerate()
    .map(|(idx, file)| {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let sanitized = sanitizer::sanitize_filename(&name);
        let indexed = sanitizer::add_sequential_prefix(&sanitized, idx + 1);
        indexed  // ← SOLO EL NOMBRE, PERDEMOS LA RUTA ORIGINAL
    })
    .collect();

// Paso 5: Distribuir en volúmenes
let volumes = distribution::distribute_files_into_volumes(&sanitized_files)?;
println!("✓ Created {} volume(s)", volumes.len());

// SIGUIENTE: Paso 6 - Verificación
// ⚠️ NUNCA se llama a distribution::generate_sync_commands()
// ⚠️ NUNCA se ejecutan comandos cp/rsync
// ⚠️ NUNCA se copian los bytes reales del disco
```

#### El Problema en Detalle

1. **Pérdida de Información de Ruta**:
   - `audio_files: Vec<PathBuf>` contiene `/home/user/music/song.mp3`
   - Se convierte a un String: `"001_song.mp3"`
   - **La información de dónde estaba el archivo se pierde para siempre**

2. **VolumeSegment No Puede Saber la Ruta Origen**:
   - `DistributedFile::destination_path` es relativa: `"VOL_01/001_song.mp3"`
   - No hay forma de decirle a `cp` dónde BUSCAR el archivo original
   - El comando generado quedaría algo como:
     ```bash
     cp 'VOL_01/001_song.mp3' '/mnt/usb/VOL_01/001_song.mp3'
     # ↑ Ruta de origen FALSA - el archivo no está en VOL_01 aún
     ```

3. **Los Comandos Nunca Se Ejecutan**:
   - `generate_sync_commands()` genera un `Vec<String>` de comandos
   - **Pero en `provision_usb()` nunca se invoca esta función**
   - Y aunque se invocara, nunca se ejecutaría con `process::Command`

#### Impacto Técnico
- **USB queda vacía** tras "provisioning" completado
- Usuario cree que la herramienta funcionó ✓ (es rápida, sin errores)
- Luego conecta USB al estéreo: **silencio absoluto**
- **Data loss potential**: Se crea backup pero los archivos nunca se copian al USB

#### Solución Requerida

**A. Cambiar la Estructura de Datos**:
```rust
// Cambiar de Vec<String> a Vec<AudioFile> que mantenga ambas rutas

#[derive(Debug, Clone)]
pub struct AudioFile {
    pub source_path: PathBuf,      // Ruta original: /home/user/music/song.mp3
    pub sanitized_name: String,    // Nombre sanitizado: 001_song.mp3
    pub destination_volume: usize, // VOL_01 = índice 1
}
```

**B. Modificar distribution.rs**:
```rust
pub fn generate_sync_commands(
    volumes: &[VolumeSegment],
    audio_files: &[AudioFile],    // ← AÑADIR aquí
    target_usb: &Path,
) -> Result<Vec<String>> {
    let mut commands = Vec::new();

    for (source, audio_file) in zip(audio_files, ...) {
        let dest = target_usb
            .join(format!("VOL_{:02}", audio_file.destination_volume))
            .join(&audio_file.sanitized_name);

        commands.push(format!(
            "cp '{}' '{}'",
            source.display(),
            dest.display()
        ));
    }
    Ok(commands)
}
```

**C. Ejecutar los Comandos en main.rs**:
```rust
// Paso 5: Distribuir y COPIAR
let volumes = distribution::distribute_files_into_volumes(&audio_files)?;
let sync_commands = distribution::generate_sync_commands(&volumes, &audio_files, usb_mount)?;

if !args.dry_run {
    for cmd in sync_commands {
        println!("Executing: {}", cmd);
        std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .output()?;
    }
}
```

---

### ❌ **PROBLEMA #2: Omisión de Filtro de Archivos Ocultos/Basura**

#### Ubicación
Línea ~174-185 en `src/main.rs` - función `scan_audio_files()`

#### Código Problemático
```rust
fn scan_audio_files(source_path: &std::path::Path) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;
    let mut files = Vec::new();
    let audio_extensions = ["mp3", "flac", "wav", "aac", "ogg", "wma"];

    for entry in WalkDir::new(source_path)
        .into_iter()
        .filter_map(|e| e.ok())
        // ⚠️ SIN FILTRO PARA DOTFILES
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if audio_extensions.contains(&ext_str.as_str()) {
                    files.push(path.to_path_buf());
                }
            }
        }
    }

    files.sort();
    Ok(files)
}
```

#### El Problema en Detalle

**Caso 1: Archivos Ocultos (dotfiles)**
```
/home/user/music/
  song.mp3              ← Válido
  .song_draft.mp3       ← ⚠️ CAPTURADO - es un dotfile macOS
  ._song.mp3            ← ⚠️ CAPTURADO - metadatos macOS (AppleDouble)
  .Trash/:
    old_song.mp3        ← ⚠️ CAPTURADO - basura
```

**Caso 2: Archivos de Sincronización**
```
  .sync/               ← Carpeta oculta de Dropbox/Google Drive
    tempo.mp3          ← ⚠️ CAPTURADO
  .fuse_*/             ← Archivos temporales del sistema
```

**Caso 3: Archivos Corruptos**
```
  ~song.mp3$           ← Lock file (~ o $)
  song.mp3.bak         ← Backup oculto creado por editor
  song.mp3~            ← Backup tilde de Emacs
```

#### Impacto Técnico

El estéreo legacy **FALLA** cuando intenta:
1. Leer metadatos ID3 de ".mp3" (archivo sin nombre)
2. Procesar AppleDouble "._song.mp3" (formato especial de macOS)
3. Parsear metadatos corruptos

**Síntomas observados**:
- Pantalla del estéreo: "ERROR PLAYING" o "CORRUPTED FILE"
- Congelación del firmware al intentar escanear directorios
- **Corrupción de lista de reproducción**

#### Solución Requerida

```rust
fn scan_audio_files(source_path: &std::path::Path) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;
    let mut files = Vec::new();
    let audio_extensions = ["mp3", "flac", "wav", "aac", "ogg", "wma"];

    for entry in WalkDir::new(source_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // 🔧 NUEVO: Filtrar dotfiles
        if let Some(file_name) = path.file_name() {
            let name_str = file_name.to_string_lossy();

            // Ignorar archivos que empiezan con "."
            if name_str.starts_with('.') {
                continue;
            }

            // Ignorar archivos temporales/backup
            if name_str.ends_with('~')
                || name_str.ends_with(".bak")
                || name_str.starts_with('~')
                || name_str.starts_with('$')
            {
                continue;
            }
        }

        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if audio_extensions.contains(&ext_str.as_str()) {
                    files.push(path.to_path_buf());
                    info!("✓ Audio file: {}", path.display());
                }
            }
        }
    }

    files.sort();
    info!("Scanned {} audio files (filtered dotfiles)", files.len());
    Ok(files)
}
```

---

### ❌ **PROBLEMA #3: Manejo de Extensiones No Resiliente**

#### Ubicación
Línea ~141-150 en `src/main.rs` - Paso 4

#### Código Problemático
```rust
let sanitized_files: Vec<String> = audio_files
    .iter()
    .enumerate()
    .map(|(idx, file)| {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        // ⚠️ name = "canción.mp3"

        let sanitized = sanitizer::sanitize_filename(&name);
        // ⚠️ Dependiendo de cómo esté implementado:
        //   - Podría ser "cancin.mp3" ✓ (correcto)
        //   - O "cancin_mp3" ✗ (PIERDE EL PUNTO)

        let indexed = sanitizer::add_sequential_prefix(&sanitized, idx + 1);
        // ⚠️ Luego le añade el prefijo
        indexed
    })
    .collect();
```

#### Auditoría de sanitizer.rs

Revisando el código actual (línea 15-27):
```rust
pub fn sanitize_filename(input: &str) -> String {
    let cleaned = SANITIZE_REGEX.replace_all(input, "");
    let truncated: String = cleaned.chars().take(32).collect();
    truncated
}
```

**El regex es**: `[^a-zA-Z0-9\.\-\_]`
- Esto significa: "permitir SOLO `[a-z A-Z 0-9 . - _]`"
- Todo lo demás se elimina

**Casos de prueba**:
```
✓ "song.mp3"      → "song.mp3"         (OK)
✓ "Canción.mp3"   → "Cancin.mp3"      (OK - tildes se quitan, punto se mantiene)
✓ "song 2.mp3"    → "song2.mp3"       (OK - espacio se quita, punto se mantiene)
? "song.MP3"      → "song.MP3"        (OK - mayúscula se mantiene)
✗ "song's.mp3"    → "songs.mp3"       (PROBLEMA: apóstrofo se quita - cambio de significado)
```

**PERO**, hay un caso que SÍ es problemático:

Línea 22 de sanitizer.rs:
```rust
let truncated: String = cleaned.chars().take(32).collect();
```

Si tenemos un archivo muy largo con extensión:
```
"aaaaaaaaaaaaaaaaaaaaaaaaaaaa.mp3"  (28 chars nombre + 4 de extensión = 32)
→ se truncada a 32 chars
→ resultado: "aaaaaaaaaaaaaaaaaaaaaaaaaaaa.mp3" (OK)

PERO:
"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.mp3"  (31 chars nombre + 4 de extensión = 35)
→ se trunca a 32 chars
→ resultado: "aaaaaaaaaaaaaaaaaaaaaaaaaaaa.mp"
# ↑ SE PIERDE ".mp3", QUEDA ".mp"
```

#### Impacto Técnico
- Dispositivos legacy esperan `.mp3`, no `.mp`
- El reproductor **falla silenciosamente** o intenta decodificar como otro formato
- Audio no se reproduce

#### Solución Requerida

Separar el nombre de la extensión ANTES de sanitizar:

```rust
pub fn sanitize_filename(input: &str) -> String {
    // Separar nombre y extensión
    let (name, ext) = if let Some(dot_pos) = input.rfind('.') {
        let (n, e) = input.split_at(dot_pos);
        (n, e)  // e incluye el punto: ".mp3"
    } else {
        (input, "")
    };

    // Sanitizar SOLO el nombre
    let cleaned_name = SANITIZE_REGEX.replace_all(name, "");

    // Volver a concatenar, respetando límite de 32 chars PARA EL NOMBRE
    // (la extensión NO cuenta en el límite)
    let name_truncated: String = cleaned_name
        .chars()
        .take(32 - ext.len().min(4))  // Reservar espacio para extensión
        .collect();

    format!("{}{}", name_truncated, ext)
}
```

**Validación**:
```
"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.mp3"  (35 chars)
→ name="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" (31 chars)
→ ext=".mp3" (4 chars)
→ name_truncated="aaaaaaaaaaaaaaaaaaa" (28 chars, reservando 4 para ext)
→ resultado: "aaaaaaaaaaaaaaaaaaa.mp3" (32 chars total) ✓
```

---

## 📋 Resumen de Correcciones Necesarias

| # | Archivo | Función | Cambio | Prioridad |
|---|---------|---------|--------|-----------|
| 1 | main.rs | provision_usb() | Llamar generate_sync_commands() y ejecutar | 🔴 CRÍTICA |
| 1b | distribution.rs | generate_sync_commands() | Aceptar rutaOriginal + sanitizedName | 🔴 CRÍTICA |
| 2 | main.rs | scan_audio_files() | Filtrar dotfiles y archivos temporales | 🔴 CRÍTICA |
| 3 | sanitizer.rs | sanitize_filename() | Separar nombre/ext antes de sanitizar | 🟠 ALTA |

---

## ✅ Plan de Remediación

### Fase 1: Proteger de Archivos Basura (CRÍTICA)
- [ ] Modificar `scan_audio_files()` para filtrar dotfiles
- [ ] Agregar tests para casos de `.` y `._` prefijos

### Fase 2: Hacer que las Copias Realmente Ocurran (CRÍTICA)
- [ ] Refactorizar estructura de datos para mantener ruta origen
- [ ] Modificar `distribution::generate_sync_commands()`
- [ ] Implementar ejecución de comandos en `provision_usb()`
- [ ] Agregar manejo de dry-run vs ejecución real
- [ ] Implementar logging de cada copia ejecutada

### Fase 3: Hardening de Extensiones (ALTA)
- [ ] Refactorizar `sanitizer::sanitize_filename()`
- [ ] Agregar test case para archivos con nombre muy largo
- [ ] Validar truncamiento respeta límite de 32 chars totales

### Fase 4: Tests de Integración
- [ ] E2E test con directorio real de música
- [ ] Verificar archivos copia bits iguales
- [ ] Verificar estructura VOL_XX/001_name.mp3

---

**Conclusión**: El CLI y la arquitectura son sólidos, pero **la lógica de I/O está incompleta**. Son 3 defectos específicos que impiden que la herramienta funcione end-to-end.
