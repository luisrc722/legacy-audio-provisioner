# Arquitectura y Diseno - Legacy Audio Provisioner

## Vision General

Legacy Audio Provisioner implementa un pipeline transaccional para preparar USBs compatibles con firmware legacy (FAT32, limites de nombres y jerarquia, recovery por checksum).

El binario operativo principal sigue una arquitectura de entrypoint delgado: la CLI inicializa runtime/logging y delega el flujo de negocio en una capa de orquestacion dedicada.

Flujo principal:

```text
Validar HW -> Descubrir -> Backup -> Sanitizar/Planificar -> Normalizar+Copia -> Verificar -> Finalizar -> Expulsión Segura
```

## Etapas del Pipeline

### 1. Validacion de hardware

**Modulo:** `crates/lap-core/src/hardware.rs`

- Valida mountpoint real contra `/proc/mounts`.
- Permite solo dispositivos de bloque removibles (`/sys/block/*/removable`).
- Requiere `vfat`/FAT32 para provisionar.

### 2. Discovery de audio

**Modulo:** `crates/lap-core/src/audio_discovery.rs`

- Escaneo recursivo con `walkdir`.
- Filtrado temprano de entradas ocultas/sistema (`.*`, `System Volume Information`, `$RECYCLE.BIN`, `FOUND.*`).
- Genera reporte con archivos soportados y metrica de tamano.

### 3. Backup y verificacion de cuota

**Modulo:** `crates/lap-core/src/backup.rs`

- Crea backup local timestamped.
- Copia con hashing SHA256 en streaming.
- Verifica espacio con `statvfs` antes de continuar.

### 4. Sanitizacion y planificacion

**Modulos:** `crates/lap-core/src/sanitizer.rs`, `crates/lap-core/src/distribution.rs`

- Sanitiza nombres y aplica prefijo secuencial.
- `distribution` es planner puro (sin I/O fisica).
- Segmenta en `VOL_XX` con maximo 50 archivos por volumen.

### 5. Orquestacion, progreso y escritura fisica

**Modulos:** `crates/lap-bin-provision/src/main.rs`, `crates/lap-bin-provision/src/orchestrator.rs`, `crates/lap-bin-provision/src/reporter.rs`, `crates/lap-core/src/normalizer.rs`

- `main.rs` actua como entrypoint delgado (parseo + bootstrap + dispatch).
- La capa `orchestrator.rs` concentra el flujo de provision/refactor/resume.
- `reporter.rs` abstrae progreso/feedback con `ProgressReporter` (CLI e IPC JSON).
- Cada archivo pasa por `normalizer::normalize_audio(...)` antes de escribir en USB.
- Se actualiza checkpoint por archivo (`InProgress`/`Completed`/`Failed`).

### 6. Checkpoint atomico y recovery

**Modulos:** `crates/lap-core/src/checkpoint.rs`, `crates/lap-core/src/recovery.rs`

- Estado persistido en `.provisioning_checkpoint` con `BTreeMap<usize, FileCheckpoint>`.
- Escritura atomica: `tmp -> sync_all -> rename`.
- `--resume` ejecuta recovery granular por divergencia SHA256.

### 7. Verificacion final y expulsion segura

**Modulo:** `crates/lap-core/src/verification.rs`

- Verifica topologia (`VOL_XX`, maximo 50 archivos, nombres validos).
- Verifica integridad contra hashes del checkpoint.
- En Linux ejecuta `sync`, `umount` y `udisksctl power-off`.

## Mapa de Modulos

```text
crates/
├── lap-core/
│   └── src/
│       ├── audio_discovery.rs
│       ├── backup.rs
│       ├── checkpoint.rs
│       ├── crypto.rs
│       ├── diffing.rs
│       ├── distribution.rs
│       ├── hardware.rs
│       ├── ingestion.rs
│       ├── journal.rs
│       ├── normalizer.rs
│       ├── recovery.rs
│       ├── sanitizer.rs
│       ├── security.rs
│       └── verification.rs
└── lap-bin-provision/
	└── src/
		├── main.rs
		├── orchestrator.rs
		└── reporter.rs
```

## Invariantes Operacionales

- No provisionar si el destino no es removible FAT32.
- No exceder 50 archivos por volumen.
- No marcar sesion `Completed` sin verificacion final exitosa.
- En recovery, no recopiado masivo: solo faltantes/corruptos.
