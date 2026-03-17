# PBT and E2E Test Plan

## Objective
Demostrar matemáticamente y empíricamente que el sistema cumple los invariantes de compatibilidad legacy (FAT32, 32-bit firmware) y resiliencia operativa bajo fallos de hardware o interrupciones violentas.

## Test Layers
- **Unit tests:** Aislamiento lógico por módulo (`src/*`).
- **Integration tests:** Ensamblaje de pipeline (`tests/integration_test.rs`).
- **PBT (Property-Based Testing):** Estrés de fuzzing contra reglas de sanitización.
- **E2E (End-to-End):** Pruebas destructivas con I/O física, interrupción de kernel y recuperación criptográfica.

## Current Baseline

- Unit tests: `41`
- Integration tests: `11`
- Doc tests: `2`
- Total: `54/54` passing

## Property-Based Testing (PBT)
**Estado actual:** Implementado y obligatorio en CI.
Se utiliza el crate `proptest` para validar matemáticamente la capa de transformación, la cual es susceptible a inputs impredecibles del usuario (nombres con emojis, caracteres asiáticos, longitudes extremas).

### Propiedades Garantizadas (Implementadas en `sanitizer.rs`)
1. **Sanitización de Nombres:**
   - **Propiedad 1:** Para cualquier `String` de entrada (UTF-8 válido o inválido), la salida es estrictamente `is_ascii()`.
   - **Propiedad 2:** La longitud total en bytes de la salida (incluyendo prefijo y extensión) es matemáticamente `<= 32`.
   - **Propiedad 3:** La extensión de destino (`.mp3`) es inmutable y siempre está presente al final de la cadena.

### Propiedades de Planificación (Implementadas en `distribution.rs`)
1. **Planificación de Volúmenes (Invariantes de Estado):**
   - **Propiedad 1:** Todo `VolumeSegment` retornado cumple rigurosamente `files.len() <= 50`.
   - **Propiedad 2:** La suma de archivos en todos los volúmenes es exactamente igual al total de archivos inyectados en el input (`sum(files) == input.len()`). Ningún archivo se pierde o se duplica en memoria.

## E2E Critical Scenario: Disaster Recovery
**Escenario:** Provisión interrumpida a nivel de kernel (`SIGKILL`) durante transcodificación FFmpeg, seguida de recuperación automática (`resume`).

**Secuencia de Validación Automática:**
1. Generar fixtures reales (`mp3` passthrough + `flac` para forzar transcodificación).
2. Iniciar provisión en background.
3. Sondear disco y enviar `SIGKILL (-9)` en el milisegundo en que el primer archivo toque la USB.
4. Auditar bitácora transaccional (estado debe ser `Completed` para el archivo 1, e `InProgress` para el archivo 2).
5. Ejecutar comando de reanudación `resume`.
6. **Aserciones Finales:**
   - Cero inodos huérfanos de 0 bytes en la tabla FAT.
   - El archivo pendiente fue recodificado exitosamente por el normalizador.
   - Ambos archivos están íntegros en la USB.
   - `SHA256(disco) == SHA256(checkpoint)`.

## Traceability Matrix
| Requirement | Evidence (Code/Tests) | Status |
| :--- | :--- | :--- |
| R-03 Sanitización <=32 ASCII | `src/sanitizer.rs` PBT suite (`proptest`) | Covered |
| R-04 Hardware lock FAT32 | `src/hardware.rs` tests (`test_parent_block_device_parsing`) | Covered |
| R-05 Backup + checksum | `src/backup.rs` tests + I/O integration test | Covered |
| R-06 Discovery filter | `src/audio_discovery.rs` tests (hidden/system block) | Covered |
| R-07 Planner <=50 | `src/distribution.rs` tests (single/multi-volume invariant) | Covered |
| R-15 Progress feedback | `src/main.rs` (bar render via `indicatif`) | Covered |
| R-16 Atomic checkpoint | `src/checkpoint.rs::test_checkpoint_atomic_save` | Covered |
| R-17 Recovery resume | `src/recovery.rs` E2E SIGKILL Runbook | Covered |
| R-19 DRM quarantine path | `tests/integration_test.rs::test_08_m4p_is_reported_as_drm_protected` | Covered |
| R-20 Read-only detection | `tests/integration_test.rs::test_09_read_only_filesystem_maps_to_typed_error` | Covered |
| R-22 NAND spoofing guard | `tests/integration_test.rs::test_10_hardware_fraud_detected_after_five_hash_mismatches` | Covered |
| R-23 Incremental sync diff | `tests/integration_test.rs::test_05_sync_diff_ignores_existing_hashes` | Covered |
| R-25/R-26 Untracked quarantine | `tests/integration_test.rs::test_06_orphan_isolation_to_quarantine` | Covered |
| R-30 Canonical path guard | `src/main.rs::validate_canonical_paths` (equality + nesting, `canonicalize`) | Covered |
| IPC contract JSON | `tests/integration_test.rs::test_07_ipc_event_serialization_contract` | Covered |
| R-T5 Final verification | `src/verification.rs` strict QA + safe eject syscalls | Covered |

## CI Execution Requirements
- Ejecución obligatoria de `cargo test` en cada Pull Request.
- Pipeline dedicado para la prueba E2E de Recuperación (`SIGKILL`). **Requisito de infraestructura:** Debe ejecutarse en un *Privileged Runner* (Linux) utilizando un `loop-mounted FAT32 fixture` para simular la I/O de una memoria USB real sin depender de hardware físico.
- Se rechaza automáticamente cualquier PR que elimine o debilite las aserciones de la Traceability Matrix.

## Manual E2E Runbook (Disaster Simulation)
Para auditar la resiliencia física de la arquitectura, ejecute este script en un entorno Linux. Destruirá el proceso a la mitad de la copia y forzará la recuperación.

```bash
# 1. Compilar y preparar entorno
cargo build
mkdir -p /tmp/legacy_source /tmp/legacy_usb

# 2. Generar fixtures (requiere ffmpeg instalado)
ffmpeg -f lavfi -i "sine=frequency=440:duration=5" -c:a libmp3lame /tmp/legacy_source/01.mp3
ffmpeg -f lavfi -i "sine=frequency=880:duration=5" -c:a flac /tmp/legacy_source/02.flac

# 3. Lanzar provisión y mutilar proceso atómicamente
./target/debug/legacy-audio-provisioner provision --usb /tmp/legacy_usb --source /tmp/legacy_source -vv &
PROV_PID=$!

# Sondear el disco para matar el proceso apenas escriba el primer archivo
for i in $(seq 1 30); do
  if ls /tmp/legacy_usb/VOL_01/*.mp3 2>/dev/null | grep -q "001_"; then
    kill -9 $PROV_PID
    break
  fi
  sleep 0.2
done

# 4. Localizar la bitácora atómica y ejecutar reanudación
BACKUP_DIR=$(ls -td ~/usb_backup_* | head -1)
./target/debug/legacy-audio-provisioner resume --usb /tmp/legacy_usb --resume "$BACKUP_DIR" -vv

# 5. Verificación Criptográfica Final
sha256sum /tmp/legacy_usb/VOL_01/*.mp3
cat "$BACKUP_DIR/.provisioning_checkpoint" | grep usb_checksum
# Ambos hashes deben coincidir matemáticamente.
```
