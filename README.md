# Legacy Audio Provisioner (LAP)

Motor de gestión de archivos para USB legacy (estéreos ~2005), escrito en Rust.
No es un script de copia: es un FS-Manager con sincronización incremental, verificación criptográfica y hardening de hardware.

[![Rust: 1.70+](https://img.shields.io/badge/Rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)
[![Architecture: Zero-Trust](https://img.shields.io/badge/Architecture-Zero--Trust-red.svg)]()
[![State: Production Ready](https://img.shields.io/badge/State-Production_Ready-success.svg)]()

## Hardware Spec

| Constraint | Regla aplicada por LAP |
| --- | --- |
| Filesystem | Solo `vfat`/FAT32 |
| Removible | Validación contra `/sys/block/*/removable` |
| Topología | `ROOT -> VOL_XX -> archivo` (máx 2 niveles) |
| Capacidad por carpeta | Máx 50 archivos |
| Nombre de archivo | ASCII, máx 32 caracteres |
| Audio destino | MP3 CBR 128-192kbps |

## System Architecture

Trazabilidad de arquitectura reciente:
- `R-01-006`: EntryPoint delgada + capa de orquestacion.
- `R-01-007`: Abstraccion de progreso desacoplada (`ProgressReporter`).
- ADR asociado: `docs/adr/0012-thin-entrypoint-orchestrator-reporter.md`.

### Estado del `src/` de raíz (retirado)

- El runtime activo del proyecto vive en los crates de workspace (`crates/lap-core`, `crates/lap-bin-provision`, `crates/lap-bin-ingest`, `crates/lap-cli-tools`).
- El código legacy del diseño monolítico fue retirado del árbol y respaldado en `backups/docs_archive_20260324_194040.tar.gz`.
- `src/` en raíz fue eliminado y ya no existe como directorio operativo.
- Para ejecución, pruebas y releases usa siempre binarios y comandos por crate (`cargo run -p ...`, `cargo test -p ...`).
- Plan de retiro controlado: ver `docs/spec/requirements_traceability.md` -> "Trabajo Residual para v0.3.1".

### 1. Sync Incremental (USB como fuente de verdad)

- `--sync` ejecuta diff SHA256 entre origen y USB.
- El checkpoint `.provisioning_checkpoint` también se espeja en la raíz de la USB.
- Se mantiene continuidad global de índices `N+1` y relleno de `VOL_XX` sin colisiones.

```mermaid
flowchart TD
    A[Scan Source] --> B[Load USB checkpoint]
    B --> C[Hash Diff Source vs USB]
    C --> D{Archivo ya existe por hash}
    D -- Si --> E[Skip]
    D -- No --> F[Assign index N+1]
    F --> G[Fill current VOL_XX up to 50]
    G --> H[Normalize + Copy]
    H --> I[Checkpoint update + mirror to USB]
```

### 2. Seguridad de hardware y transaccionalidad

- Exclusión mutua por lock físico `.lap_provisioning.lock` (PID-based, orphan tolerant).
- Dirty-bit test (`assert_rw_filesystem`) antes de procesar para detectar `EROFS`/solo lectura.
- Detección de fraude NAND: aborto con `HARDWARE_FRAUD_DETECTED` tras 5 mismatches SHA256 consecutivos.
- Checkpoint atómico POSIX: `.tmp -> sync_all() -> rename()` + `dir sync`.

### 3. Normalización y sanitización estricta

- `normalizer.rs`:
  - passthrough seguro para MP3 CBR compatible,
  - transcodificación forzada a MP3 CBR 128k si no cumple,
  - limpieza agresiva (`-map 0:a:0`, `-map_metadata -1`) para evitar bloqueos por carátulas/tags.
- `sanitizer.rs`:
  - ASCII-only,
  - límite 32 chars,
  - preservación de extensión `.mp3` con truncamiento del stem (no rompe extensión).

### 4. Gestión de integridad por cuarentena

- Archivos `untracked` en USB no se borran por defecto.
- Flujo `backup-first`: primero copia a Host, luego aislamiento en `.legacy_quarantine/<session>/`.
- Resultado: USB limpia para el estéreo, sin riesgo de pérdida de datos del cliente.

## Safety First

LAP aplica protección multicapa para minimizar riesgo operativo:

1. Origen en host tratado como solo lectura.
2. Backup local en Host con verificación SHA256.
3. Aislamiento de huérfanos en `.legacy_quarantine/` en USB (no destructivo).

## Typed Errors + IPC

Los errores de dominio viven en `ProvisioningError` y se traducen a códigos estables para frontend/operación:

- `CONCURRENCY_ERROR`
- `FILESYSTEM_READ_ONLY`
- `ENOSPC_ERROR`
- `HARDWARE_FRAUD_DETECTED`
- `DRM_PROTECTED`
- `PROVISIONING_FAILED`

Eventos IPC JSON (`crates/lap-core/src/ipc.rs`) disponibles para UI:

- `PROGRESS`
- `WARNING`
- `FATAL_ERROR`
- `SUCCESS`

## CLI Usage

```bash
# Descubrir dispositivos válidos (binary de provision)
cargo run -p lap-bin-provision -- list

# Escanear audio en la primera USB detectada
cargo run -p lap-bin-provision -- scan

# Simulación sin mutación
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/user/USB_TARGET \
  --source ~/Music \
  --dry-run

# Provisión completa
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/user/USB_TARGET \
  --source ~/Music

# Sincronización incremental
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/user/USB_TARGET \
  --source ~/Music \
  --sync

# Eventos IPC JSON
cargo run -p lap-bin-provision -- \
  --json \
  provision \
  --usb /media/user/USB_TARGET \
  --source ~/Music \
  --sync

# Reanudación tras fallo
cargo run -p lap-bin-provision -- \
  resume \
  --usb /media/user/USB_TARGET \
  --resume ~/usb_backup_20260315_1430

# Ingesta dedicada
cargo run -p lap-bin-ingest -- --help

# Herramientas auxiliares
cargo run -p lap-cli-tools -- --help
```

## Developer QA

Estado actual de calidad verificado:

- 55 unit tests
- 23 integration tests
- 2 doc tests
- Total: 80/80 passing

Runbook QA:

```bash
cargo test
cargo test -p lap-core --test integration_test
```

Cobertura relevante de Fase 2:

- diff incremental por hash
- cuarentena backup-first de huérfanos
- contrato IPC JSON
- errores tipados (`DRM_PROTECTED`, `FILESYSTEM_READ_ONLY`, `HARDWARE_FRAUD_DETECTED`, `ENOSPC_ERROR`)

## Documentation

- [Documentation Index](docs/README.md)
- [Tech Spec](docs/spec/tech_spec.md)
- [Requirements Traceability](docs/spec/requirements_traceability.md)
- [Design by Contract](docs/contracts/design_by_contract.md)
- [ADR History (Immutable, Canonical)](docs/adr/)
- [Release Checklist](CHECKLIST.md)

## Build

```bash
cargo build --release
```

## License

MIT
