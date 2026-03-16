# Integration Tests - Legacy Audio Provisioner

**Fecha**: 16 de Marzo de 2026
**Estado**: ✅ COMPLETO — 11 integration tests + 41 unit tests + 2 doc tests = **54/54 PASSING**

## Objetivo

Validar el comportamiento integrado del motor sobre los flujos críticos de Fase 2:

- sincronización incremental por hash,
- cuarentena no destructiva de huérfanos,
- contrato IPC JSON,
- errores tipados para condiciones de fallo realistas,
- invariantes legacy de nombres, volúmenes y topología.

## Suite Summary

| Test | Propósito | Estado |
|---|---|---|
| `test_00_system_dependencies` | Verifica presencia de `ffmpeg` en entorno | ✅ PASS |
| `test_01_real_sanitization_and_distribution` | R-03/R-07: sanitización + distribución 50/volumen | ✅ PASS |
| `test_02_real_audio_discovery` | Filtrado AppleDouble y carpetas ocultas | ✅ PASS |
| `test_03_real_checkpoint_tracking` | Checkpoint atómico con progreso | ✅ PASS |
| `test_04_end_to_end_backup_integration` | Backup + SHA256 en flujo real | ✅ PASS |
| `test_05_sync_diff_ignores_existing_hashes` | Diff incremental: skip de archivos existentes por hash | ✅ PASS |
| `test_06_orphan_isolation_to_quarantine` | Cuarentena backup-first en `.legacy_quarantine/` | ✅ PASS |
| `test_07_ipc_event_serialization_contract` | Contrato estructural JSON de `IpcEvent` | ✅ PASS |
| `test_08_m4p_is_reported_as_drm_protected` | DRM tipado (`DRM_PROTECTED`) para `.m4p` | ✅ PASS |
| `test_09_read_only_filesystem_maps_to_typed_error` | Dirty-bit/read-only -> `FILESYSTEM_READ_ONLY` | ✅ PASS |
| `test_10_hardware_fraud_detected_after_five_hash_mismatches` | Detección de fraude NAND -> `HARDWARE_FRAUD_DETECTED` | ✅ PASS |

## Cobertura por componente

| Componente | Cobertura de integración |
|---|---|
| `sanitizer.rs` | nombres ASCII, longitud <=32, extensión preservada |
| `distribution.rs` | segmentación `VOL_XX` con límite 50 |
| `audio_discovery.rs` | exclusión de `._*`, `.Trash`, ruido de sistema |
| `checkpoint.rs` | persistencia y progreso por archivo |
| `backup.rs` | copia y verificación SHA256 |
| `diffing.rs` | diff por hash + cuarentena backup-first |
| `ipc.rs` | serialización de eventos (`PROGRESS`, etc.) |
| `normalizer.rs` | detección tipada de DRM por extensión/inspección |
| `hardware.rs` | mapeo de fs read-only a error tipado |
| `verification.rs` | fraude de hardware por mismatches SHA256 consecutivos |

## Coverage Baseline

```text
Unit tests:         41
Integration tests:  11
Doc tests:           2
--------------------------------
TOTAL:              54/54 PASSING
```

## Ejecutar la suite

```bash
# Todo
cargo test

# Solo integración
cargo test --test integration_test

# Integración con logs
cargo test --test integration_test -- --nocapture
```

## Notas de diseño

- Los tests de integración usan APIs reales del crate (sin mocks de lógica core).
- El aislamiento se realiza con `tempfile::TempDir` para no contaminar el host.
- La cobertura de errores tipados asegura frontera estable para frontend IPC.

## Estado documental

`docs/testing/integration_tests.md` está alineado con el estado actual del repositorio y debe actualizarse junto con cualquier cambio en `tests/integration_test.rs`.
