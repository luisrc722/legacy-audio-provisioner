# Arquitectura y Diseno - Legacy Audio Provisioner

## Vision General

Legacy Audio Provisioner implementa un pipeline transaccional para preparar USBs compatibles con firmware legacy (FAT32, limites de nombres y jerarquia, recovery por checksum).

Flujo principal:

```text
Validate HW -> Discover -> Backup -> Sanitize/Plan -> Normalize+Copy -> Verify -> Finalize -> Safe Eject
```

## Etapas del Pipeline

### 1. Validacion de hardware

**Modulo:** `src/hardware.rs`

- Valida mountpoint real contra `/proc/mounts`.
- Permite solo dispositivos de bloque removibles (`/sys/block/*/removable`).
- Requiere `vfat`/FAT32 para provisionar.

### 2. Discovery de audio

**Modulo:** `src/audio_discovery.rs`

- Escaneo recursivo con `walkdir`.
- Filtrado temprano de entradas ocultas/sistema (`.*`, `System Volume Information`, `$RECYCLE.BIN`, `FOUND.*`).
- Genera reporte con archivos soportados y metrica de tamano.

### 3. Backup y verificacion de cuota

**Modulo:** `src/backup.rs`

- Crea backup local timestamped.
- Copia con hashing SHA256 en streaming.
- Verifica espacio con `statvfs` antes de continuar.

### 4. Sanitizacion y planificacion

**Modulos:** `src/sanitizer.rs`, `src/distribution.rs`

- Sanitiza nombres y aplica prefijo secuencial.
- `distribution` es planner puro (sin I/O fisica).
- Segmenta en `VOL_XX` con maximo 50 archivos por volumen.

### 5. Escritura fisica y normalizacion

**Modulos:** `src/main.rs`, `src/normalizer.rs`

- La copia fisica se ejecuta en el orquestador (`main.rs`).
- Cada archivo pasa por `normalizer::normalize_audio(...)` antes de escribir en USB.
- Se actualiza checkpoint por archivo (`InProgress`/`Completed`/`Failed`).

### 6. Checkpoint atomico y recovery

**Modulos:** `src/checkpoint.rs`, `src/recovery.rs`

- Estado persistido en `.provisioning_checkpoint` con `BTreeMap<usize, FileCheckpoint>`.
- Escritura atomica: `tmp -> sync_all -> rename`.
- `--resume` ejecuta recovery granular por divergencia SHA256.

### 7. Verificacion final y expulsion segura

**Modulo:** `src/verification.rs`

- Verifica topologia (`VOL_XX`, maximo 50 archivos, nombres validos).
- Verifica integridad contra hashes del checkpoint.
- En Linux ejecuta `sync`, `umount` y `udisksctl power-off`.

## Mapa de Modulos

```text
src/
├── lib.rs
├── main.rs
├── hardware.rs
├── audio_discovery.rs
├── backup.rs
├── sanitizer.rs
├── distribution.rs
├── normalizer.rs
├── checkpoint.rs
├── recovery.rs
└── verification.rs
```

## Invariantes Operacionales

- No provisionar si el destino no es removible FAT32.
- No exceder 50 archivos por volumen.
- No marcar sesion `Completed` sin verificacion final exitosa.
- En recovery, no recopiado masivo: solo faltantes/corruptos.
