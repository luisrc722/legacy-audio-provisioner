# Plan de Pruebas PBT y E2E

## Objetivo
Demostrar matemáticamente y empíricamente que el sistema cumple los invariantes de compatibilidad legacy (FAT32, 32-bit firmware) y resiliencia operativa bajo fallos de hardware o interrupciones violentas.

## Capas de Prueba
- **Unit tests:** Aislamiento lógico por módulo (`crates/lap-core/src/*`).
- **Pruebas de integración:** Ensamblaje de pipeline (`crates/lap-core/tests/integration_test.rs`).
- **PBT (Property-Based Testing):** Estrés de fuzzing contra reglas de sanitización.
- **E2E (End-to-End):** Pruebas destructivas con I/O física, interrupción de kernel y recuperación criptográfica.

## Línea Base Actual

- Unit tests: `55`
- Pruebas de integración: `20`
- Doc tests: `2`
- Total: `77/77` exitosas

## Telemetría de I/O para R-02-010 (Mitigación de Desgaste NAND)
**Estado actual:** Instrumentación disponible y reproducible. Pendiente registrar corrida baseline-vs-actual para subir a `VERIFIED`.

### Evidencia Instrumentada
- Script: `scripts/telemetry_r02_010_io_wear.sh`
- Método: `strace -c -e trace=fsync` sobre corrida masiva de provisioning (500 archivos)
- Métricas extraídas: total de llamadas `fsync`, ratio `fsync/archivo`, latencia total del lote

### Criterios de Aceptación
1. Ratio de `sync_all()` por archivo <= `0.1`.
2. Ausencia de patrón `sync_all()` por cada iteración de archivo.
3. Reducción >= `20%` del p95 de latencia frente al baseline legacy.

### Ejecución Reproducible
```bash
chmod +x scripts/telemetry_r02_010_io_wear.sh
./scripts/telemetry_r02_010_io_wear.sh <usb_mount_point>
```

### Nota Operativa
La medición requiere un mountpoint real de dispositivo de bloque (FAT32/removible). Rutas locales temporales no pasan la validación estricta de hardware por diseño.

### Resultado de Corrida Real (2026-03-25)
- Dataset real: biblioteca del usuario (`Found 1692 audio files`) desde `docs/testing/evidence/r02_010/2026-03-25/stdout.log`.
- Comando de medición: `strace -c -e trace=fsync`.
- Telemetría del kernel (`docs/testing/evidence/r02_010/2026-03-25/strace_summary.txt`): `8913` llamadas `fsync`.
- Ratio observado: `8913 / 1692 = 5.267`.
- Umbral objetivo R-02-010: `<= 0.1`.
- Dictamen: **NO CONFORME**. La corrida real demuestra que aún existe amplificación de escrituras y el requisito no puede marcarse como `VERIFIED`.

### Evidencia de Archivo
- `docs/testing/evidence/r02_010/2026-03-25/strace_summary.txt` (métrica de syscalls del kernel)
- `docs/testing/evidence/r02_010/2026-03-25/stdout.log` (conteo de archivos y contexto de ejecución)

## Pruebas Basadas en Propiedades (PBT)
**Estado actual:** Implementado y obligatorio en CI.
Se utiliza el crate `proptest` para validar matemáticamente la capa de transformación, la cual es susceptible a inputs impredecibles del usuario (nombres con emojis, caracteres asiáticos, longitudes extremas).

### Propiedades Garantizadas (Implementadas en `sanitizer.rs`)
1. **Sanitización de Nombres:**
   - **Propiedad 1:** Para cualquier `String` de entrada (UTF-8 válido o inválido), la salida es estrictamente `is_ascii()`.
   - **Propiedad 2:** La longitud total en bytes de la salida (incluyendo prefijo y extensión) es matemáticamente `<= 32`.
   - **Propiedad 3:** La extensión de destino (`.mp3`) es inmutable y siempre está presente al final de la cadena.

### Propiedades de Planificacion (Implementadas en `distribution.rs`)
1. **Planificación de Volúmenes (Invariantes de Estado):**
   - **Propiedad 1:** Todo `VolumeSegment` retornado cumple rigurosamente `files.len() <= 50`.
   - **Propiedad 2:** La suma de archivos en todos los volúmenes es exactamente igual al total de archivos inyectados en el input (`sum(files) == input.len()`). Ningún archivo se pierde o se duplica en memoria.

## Escenario Critico E2E: Recuperacion ante Desastres
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

## Matriz de Trazabilidad
| Requisito | Evidencia (Codigo/Pruebas) | Estado |
| :--- | :--- | :--- |
| R-03 Sanitización <=32 ASCII | `crates/lap-core/src/sanitizer.rs` suite PBT (`proptest`) | Cubierto |
| R-04 Lock de hardware FAT32 | `crates/lap-core/src/hardware.rs` pruebas (`test_parent_block_device_parsing`) | Cubierto |
| R-05 Backup + checksum | `crates/lap-core/src/backup.rs` pruebas + prueba de integración de I/O | Cubierto |
| R-06 Filtro de descubrimiento | `crates/lap-core/src/audio_discovery.rs` pruebas (bloqueo hidden/system) | Cubierto |
| R-07 Planificador <=50 | `crates/lap-core/src/distribution.rs` pruebas (invariante mono/multi-volumen) | Cubierto |
| R-15 Feedback de progreso | `crates/lap-bin-provision/src/reporter.rs` + `crates/lap-bin-provision/src/orchestrator.rs` | Cubierto |
| R-16 Checkpoint atómico | `crates/lap-core/src/checkpoint.rs::test_checkpoint_atomic_save` | Cubierto |
| R-17 Reanudación de recuperación | `crates/lap-core/src/recovery.rs` runbook E2E con SIGKILL | Cubierto |
| R-19 Ruta de cuarentena DRM | `crates/lap-core/tests/integration_test.rs::test_08_m4p_is_reported_as_drm_protected` | Cubierto |
| R-20 Detección read-only | `tests/integration_test.rs::test_09_read_only_filesystem_maps_to_typed_error` | Cubierto |
| R-22 Guardia de spoofing NAND | `tests/integration_test.rs::test_10_hardware_fraud_detected_after_five_hash_mismatches` | Cubierto |
| R-23 Diff incremental sync | `tests/integration_test.rs::test_05_sync_diff_ignores_existing_hashes` | Cubierto |
| R-25/R-26 Cuarentena de no rastreados | `tests/integration_test.rs::test_06_orphan_isolation_to_quarantine` | Cubierto |
| R-30 Guardia de rutas canónicas | `crates/lap-bin-provision/src/orchestrator.rs::validate_canonical_paths` (igualdad + anidamiento, `canonicalize`) | Cubierto |
| Contrato JSON IPC | `tests/integration_test.rs::test_07_ipc_event_serialization_contract` | Cubierto |
| R-T5 Verificación final | `crates/lap-core/src/verification.rs` QA estricto + syscalls de expulsión segura | Cubierto |

## Requisitos de Ejecución en CI
- Ejecución obligatoria de `cargo test` en cada Pull Request.
- Pipeline dedicado para la prueba E2E de Recuperación (`SIGKILL`). **Requisito de infraestructura:** Debe ejecutarse en un *Privileged Runner* (Linux) utilizando un `loop-mounted FAT32 fixture` para simular la I/O de una memoria USB real sin depender de hardware físico.
- Se rechaza automáticamente cualquier PR que elimine o debilite las aserciones de la Traceability Matrix.

## Runbook Manual E2E (Simulación de Desastre)
Para auditar la resiliencia física de la arquitectura, ejecute este script en un entorno Linux. Destruirá el proceso a la mitad de la copia y forzará la recuperación.

```bash
# 1. Compilar y preparar entorno
cargo build -p lap-bin-provision
mkdir -p /tmp/legacy_source /tmp/legacy_usb

# 2. Generar fixtures (requiere ffmpeg instalado)
ffmpeg -f lavfi -i "sine=frequency=440:duration=5" -c:a libmp3lame /tmp/legacy_source/01.mp3
ffmpeg -f lavfi -i "sine=frequency=880:duration=5" -c:a flac /tmp/legacy_source/02.flac

# 3. Lanzar provisión y mutilar proceso atómicamente
cargo run -p lap-bin-provision -- provision --usb /tmp/legacy_usb --source /tmp/legacy_source -vv &
PROV_PID=$!

# Sondear el disco para matar el proceso apenas escriba el primer archivo
for i in $(seq 1 30); do
   if ls /tmp/legacy_usb/VOL_01/*.mp3 2>/dev/null | grep -q "0001_"; then
    kill -9 $PROV_PID
    break
  fi
  sleep 0.2
done

# 4. Localizar la bitácora atómica y ejecutar reanudación
BACKUP_DIR=$(ls -td ~/usb_backup_* | head -1)
cargo run -p lap-bin-provision -- resume --usb /tmp/legacy_usb --resume "$BACKUP_DIR" -vv

# 5. Verificación Criptográfica Final
sha256sum /tmp/legacy_usb/VOL_01/*.mp3
cat "$BACKUP_DIR/.provisioning_checkpoint" | grep usb_checksum
# Ambos hashes deben coincidir matemáticamente.
```
