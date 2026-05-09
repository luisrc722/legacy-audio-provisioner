# Trazabilidad de Requisitos

<!-- Versionado: se rastrea aquí, no en el nombre del archivo. Usa git log para el historial completo. -->
| Campo | Valor |
|---|---|
| **Línea base actual** | v0.3.0 |
| **Última actualización** | 2026-03-24 |
| **Próxima revisión** | en cada cambio funcional que agregue, elimine o redefina un requisito |

## Alcance
Taxonomía oficial de requisitos para Legacy Audio Provisioner usando IDs jerárquicos `R-CC-NNN`.

Reglas de nomenclatura:
- `CC`: categoría del requisito (dominio de responsabilidad)
- `NNN`: ranura de secuencia dentro de la categoría
- Los IDs heredados (`R-10`, `R-33`, etc.) se conservan solo como referencia cruzada

Justificación de huecos no secuenciales:
- Las categorías son intencionalmente dispersas para permitir inserciones futuras sin renumerar.
- Las categorías de gobernanza y cumplimiento (`10`, `15`, `20`, `25`) se rastrean explícitamente y pueden ampliarse sin renumeración.

## Mapa de Categorías (Listo para Auditoría)

| Categoría | Dominio |
|---|---|
| `01` | Arquitectura y Orquestación |
| `02` | Integridad de Hardware e I/O Física (incluye Resiliencia y Tolerancia a Fallos) |
| `05` | Endurecimiento de Seguridad |
| `06` | Criptografía e Integridad de Datos |
| `08` | Lógica de Procesamiento de Audio |
| `09` | Topología y Estado Transaccional (incluye Checkpoint y Recuperación) |
| `10` | Privacidad y Protección de Datos |
| `15` | Cumplimiento Legal y de Licencias |
| `20` | Calidad de Software (QA y Pruebas) |
| `25` | Documentación y Gobernanza |

## Catálogo de Gobernanza de Ingeniería

Las siguientes categorías se mantienen para auditorías profesionales y trazabilidad forense. Algunas categorías están implementadas en `v0.3.0`, y otras se definen como alcance de gobernanza para versiones futuras.

| Categoría | Enfoque de Dominio | Controles Típicos |
|---|---|---|
| `01` | Arquitectura y Diseño de Sistema | Límites del workspace, contratos de API/traits, contratos IPC |
| `02` | Hardware e I/O Física + Resiliencia | Restricciones FAT32, protecciones de medios removibles, invariantes de I/O física, locks de concurrencia, mecanismos de tolerancia a fallos, detección de fraude de hardware |
| `05` | Seguridad de Aplicación | Prevención de path traversal, validación de entrada, mitigación de inyección |
| `06` | Criptografía e Integridad | Política de hashing, checksums, restricciones de manejo de secretos |
| `08` | Estándares de Procesamiento Multimedia | Compatibilidad de loudness/codificación, perfiles de metadatos, detección DRM |
| `09` | Persistencia y Transacciones + Checkpoint/Recuperación | Journaling, idempotencia, reconciliación de estado, checkpoints atómicos, recuperación granular |
| `10` | Privacidad y Protección de Datos | Minimización de PII, política de retención y borrado |
| `15` | Legal y Licencias | Atribución OSS, conciencia sobre patentes de códecs, controles ToS/EULA |
| `20` | QA y Pruebas | Evidencia de unit/integration/fuzz/perf y puertas de calidad |
| `25` | Documentación y Gobernanza | Ciclo de vida ADR, docs-as-code, política de contribución |

## Protocolo de Trazabilidad Bidireccional

Este repositorio opera bajo **trazabilidad bidireccional**:

1. Cada requisito de esta matriz debe apuntar al menos a un ancla de implementación en Rust o a un documento rector.
2. Cada función Rust que implemente una garantía normativa debe referenciar su ID `R-CC-NNN` en un comentario de documentación.
3. Un requisito puede pasar a `VERIFIED` solo cuando `docs/testing/integration_tests.md` documenta una prueba de integración que lo ejercita.
4. Para las categorías `02` (Hardware) y `05` (Seguridad), `VERIFIED` requiere adicionalmente evidencia negativa, adversarial o con inyección de fallos; la evidencia de ruta nominal por sí sola es insuficiente.

### Estados del Ciclo de Vida del Requisito

| Estado | Significado |
|---|---|
| `PROPOSED` | Definido en gobernanza/especificación pero no implementado completamente en código. |
| `IMPLEMENTED` | Existe código y/o política operativa, pero todavía no se registró evidencia de prueba de integración calificante. |
| `VERIFIED` | Implementado y cubierto por una prueba registrada en `docs/testing/integration_tests.md`; para las categorías `02` y `05`, la evidencia debe incluir escenarios negativos/adversariales o con inyección de fallos. |
| `DEPRECATED` | Se conserva solo por razones históricas/de auditoría; ya no es autoritativo para trabajo futuro. |

## Matriz de Requisitos (`R-CC-NNN`)

### Categoría 01: Arquitectura y Orquestación

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-01-001 | N/A | Workspace de Rust | Proyecto dividido en crates de workspace con gobernanza compartida de dependencias. | `Cargo.toml`, `crates/*/Cargo.toml` |
| R-01-002 | N/A | IPC JSON Lines | Los componentes deben emitir eventos JSON legibles por máquina para integración/debug. | `crates/lap-core/src/ipc.rs` |
| R-01-003 | R-04 | Parseo de Argumentos CLI | Parseo robusto de comandos con ayuda autogenerada y subcomandos. | `crates/lap-bin-provision/src/main.rs`, `crates/lap-bin-ingest/src/main.rs` |
| R-01-004 | R-12 (SDD) | Modo Dry Run | El flag `--dry-run` ejecuta el pipeline completo en simulación: calcula plan, emite eventos, no escribe nada. | `crates/lap-bin-provision/src/orchestrator.rs` (`provision_usb`, `dry_run_no_backup`) |
| R-01-005 | R-14 (SDD, Fase 2) | Logging Estructurado | Emitir archivo de log estructurado por dispositivo (cuando hay `--usb`) o por operación (cuando no hay USB explícita), con timestamp por evento. | `crates/lap-bin-provision/src/main.rs` (`init_session_logger`, `log_session_event`) |
| R-01-006 | N/A | EntryPoint Delgada y Orquestación | El binario CLI debe limitarse a bootstrap + dispatch y delegar el flujo de negocio a una capa de orquestación dedicada. | `crates/lap-bin-provision/src/main.rs`, `crates/lap-bin-provision/src/orchestrator.rs` |
| R-01-007 | N/A | Abstracción de Progreso | La capa de negocio reporta avance/estado vía trait para soportar implementaciones CLI/JSON sin acoplamiento. | `crates/lap-bin-provision/src/reporter.rs` |

### Categoría 02: Integridad de Hardware e I/O Física

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-02-001 | R-11 | Validador FAT32 | El FS de destino debe ser compatible (`vfat`/FAT32) y, cuando sea detectable desde el boot sector, validar allocation unit de 32 KB para firmware legacy; de lo contrario abortar. | `crates/lap-core/src/hardware.rs` (`is_valid_for_provisioning`, `validate_legacy_format_profile`) |
| R-02-002 | R-20 | Prueba de Dirty Bit | Sonda de escribibilidad física previa al provisionamiento para detectar FS de solo lectura/corrupto. | `crates/lap-core/src/hardware.rs` (`assert_rw_filesystem`) |
| R-02-003 | R-18 | Bloqueo Exclusivo | Bloqueo persistente de USB para prevenir escrituras concurrentes (control de `race condition`). | `crates/lap-core/src/hardware.rs` (`ProvisioningLock`) |
| R-02-004 | R-12 | Descubrimiento Recursivo | Escaneo recursivo con filtrado de basura oculta/de sistema antes del descenso. | `crates/lap-core/src/audio_discovery.rs` |
| R-02-005 | N/A | Detección Temprana de Dirty Bit FAT32 (Fase 2) | Prueba de escritura pre-flight (`.fat32_dirty_test`) para fallar rápido en filesystems de solo lectura. | `crates/lap-bin-provision/src/main.rs` (pre-flight check) |
| R-02-006 | N/A | Manejador ENOSPC | Detectar y salir limpiamente ante disco lleno durante escrituras de checkpoint en host. | `crates/lap-core/src/checkpoint.rs` (captura de error) |
| R-02-007 | N/A | Detección de Fraude NAND | Detectar suplantación de hardware vía mismatches SHA256 consecutivos (>5 fallos). | `crates/lap-core/src/verification.rs`, `crates/lap-core/src/recovery.rs` |
| R-02-008 | R-03 | Sanitización de Nombre de Archivo | Transformar cualquier entrada UTF-8 a solo ASCII, eliminar caracteres ilegales y signos de operacion/simbolos no alfanumericos similares, y forzar longitud total <=32 chars incluyendo prefijo secuencial (`NNN_`) y extensión `.mp3`. | `crates/lap-core/src/sanitizer.rs` (`sanitize_filename`, `add_sequential_prefix`) |
| R-02-009 | R-13 (Fase 2) | Chequeo de Salud S.M.A.R.T. Lite | Consultar estado de salud USB antes de provisionar; abortar si la bandera read-only de wear-levelling está activa. | `crates/lap-core/src/hardware.rs` (`assert_hardware_health`), `crates/lap-bin-provision/src/orchestrator.rs` (`provision_usb`, `resume_provisioning`) |
| R-02-010 | N/A | Mitigación de Desgaste NAND / Optimización I/O | En escritura USB, prohibir flush global por archivo y consolidar sincronización por frontera transaccional/volumen y eject seguro. | `crates/lap-bin-provision/src/orchestrator.rs`, `crates/lap-core/src/verification.rs` |

### Categoría 05: Endurecimiento de Seguridad

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-05-001 | R-34 | Jaula de Rutas | Bloquear path traversal vía validación de componentes y checks de contención. | `crates/lap-core/src/security.rs` (`validate_path_containment`) |
| R-05-002 | R-35 | Sanitizador de Shell | Rechazar caracteres de control para prevenir command injection en llamadas a subprocesos. | `crates/lap-core/src/security.rs` (`validate_shell_safe_filename`) |
| R-05-003 | R-36 | Sandbox de Metadatos | Limitar parseo de metadatos por límites de tamaño para mitigar vectores DoS de RAM del host. | `crates/lap-core/src/security.rs` (`validate_metadata_bomb_safety`) |

### Categoría 06: Criptografía e Integridad de Datos

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-06-001 | R-23/26 | Política de Hashing de Contenido | Identidad de contenido basada en SHA256 con helper centralizado y reutilizable para sincronización incremental y checks de integridad. | `crates/lap-core/src/crypto.rs`, `crates/lap-core/src/diffing.rs`, `crates/lap-core/src/journal.rs`, `crates/lap-core/src/recovery.rs` |
| R-06-002 | R-08 | Verificación Post-Escritura | Verificación de checksum entre artefactos esperados y objetivo después de escrituras/movimientos. | `crates/lap-core/src/verification.rs`, `crates/lap-core/src/recovery.rs` |
| R-06-003 | N/A | Higiene de Secretos (Reservado) | Política de gestión de secretos criptográficos reservada para manifiestos firmados futuros. | `Reservado para v0.3.1+` |
| R-06-004 | R-05 | Backup con Checksums | Crear backup local estable por dispositivo en host antes de cualquier operación destructiva; verificar SHA256 de cada archivo copiado. | `crates/lap-core/src/backup.rs` (`new_for_target`, `verify_backup`) |

### Categoría 08: Lógica de Procesamiento de Audio

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-08-001 | R-10 | Límite de Volumen | Límite estricto de 50 archivos por segmento `VOL_XX` para estabilidad de firmware heredado. | `crates/lap-core/src/distribution.rs` (`MAX_FILES_PER_FOLDER`) |
| R-08-002 | R-27 | Normalización de Loudness | Normalizar/estandarizar el perfil de salida de audio para nivel de reproducción consistente. | `crates/lap-core/src/normalizer.rs` |
| R-08-003 | R-21 | Extracción de Metadatos | Extraer metadatos técnicos (codec/bitrate/sample-rate) antes de la política de transformación. | `crates/lap-core/src/normalizer.rs` (`analyze_audio`) |
| R-08-004 | R-15 | Retroalimentación de UI Nativa | Señalización de progreso en tiempo real vía barras de progreso y eventos IPC. | `crates/lap-bin-provision/src/main.rs`, `crates/lap-core/src/ipc.rs` |
| R-08-005 | N/A | Detección DRM y Cuarentena (Fase 2) | Detectar audio protegido con DRM vía metadatos de ffprobe; excluir del pipeline de normalización. | `crates/lap-core/src/normalizer.rs` (`analyze_audio`, `is_drm_protected`) |
| R-08-006 | R-11 (SDD) | Eliminación de Metadatos ID3 | Lectura explícita de posibles portadas embebidas (APIC/covr/attached_pic) vía ffprobe y purga destructiva de etiquetas ID3v2 + imágenes embebidas vía `ffmpeg -map 0:a:0 -map_metadata -1` para prevenir agotamiento de memoria de firmware. | `crates/lap-core/src/normalizer.rs` (`analyze_audio`, detección de portada, flags ffmpeg) |

### Categoría 09: Topología y Estado Transaccional

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-09-001 | R-31 | Refactor In-Situ | Composición de flujo de trabajo: ingest + `provision --sync`. | `crates/lap-bin-provision/src/main.rs` (`refactor_usb`) |
| R-09-002 | R-30 | Jaula Pre-Ingesta | Checks de ruta canónica para prevenir staging dentro del montaje USB objetivo. | `crates/lap-bin-provision/src/main.rs` (`validate_canonical_paths`) |
| R-09-003 | R-32 | Guardia de Topología | Los hash matches son válidos solo cuando la topología objetivo cumple legado. | `crates/lap-core/src/diffing.rs` (`is_legacy_compliant_target`) |
| R-09-004 | R-33 | Refactor In-Place | Mover/renombrar in-place con semántica transaccional de reanudación por journal. | `crates/lap-core/src/journal.rs`, `crates/lap-bin-provision/src/main.rs` |
| R-09-005 | R-23/26 | Sync SHA256 | Diferenciación incremental de contenido para evitar escrituras redundantes en flash. | `crates/lap-core/src/diffing.rs` (`calculate_sync_diff`) |
| R-09-006 | R-25/26 | Cuarentena Segura | Aislamiento backup-first de archivos no rastreados en `.legacy_quarantine`. | `crates/lap-core/src/diffing.rs` (`quarantine_untracked_files`) |
| R-09-007 | N/A | Sistema de Checkpoint Atómico | Checkpoint JSON atómico POSIX con seguimiento de estado `BTreeMap` para tolerancia a fallos. | `crates/lap-core/src/checkpoint.rs` (`save_to_disk`, `load_from_disk`) |
| R-09-008 | N/A | Recuperación Granular | Reanudar desde checkpoint con verificación SHA256 para omitir archivos ya válidos. | `crates/lap-core/src/recovery.rs` (`execute_recovery`, `verify_usb_file`) |
| R-09-009 | R-07 | Planificador de Distribución | Algoritmo puro en memoria para bucketing: agrupa archivos sanitizados en segmentos `VOL_XX` (<=50 archivos c/u) sin efectos colaterales de I/O; cero archivos perdidos o duplicados (invariante). | `crates/lap-core/src/distribution.rs` (`plan_distribution`, `VolumeSegment`) |
| R-09-010 | R-T5 | Verificación Final + Expulsión Segura | Auditoría de topología previa a expulsión (VOL_XX, <=50, ASCII, <=32 chars) + SHA256 contra checkpoint, luego `sync` -> `umount` -> `udisksctl power-off`. | `crates/lap-core/src/verification.rs` (`pre_eject_verification`, `safe_eject`) |
| R-09-011 | N/A | Barrido Universal de Cuarentena en Raíz | Antes de procesar deltas, aislar en `.legacy_quarantine` cualquier nodo raíz fuera de whitelist (`VOL_XX`, checkpoint, lock, basura de sistema permitida). | `crates/lap-core/src/diffing.rs` (`collect_non_whitelisted_root_entries`, `quarantine_non_whitelisted_root_entries`), `crates/lap-bin-provision/src/orchestrator.rs` |

### Categoría 10: Privacidad y Protección de Datos

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-10-001 | N/A | Minimización de Logs | Evitar almacenar datos identificables del usuario innecesarios en logs. | `Política + puerta de revisión de código` |
| R-10-002 | N/A | Ciclo de Vida de Datos Temporales | Definir política de retención/borrado para staging y artefactos temporales de backup. | `crates/lap-bin-provision/src/main.rs`, política operativa |
| R-10-003 | N/A | Manejo de PII (Reservado) | Reservado para clasificación explícita de PII y controles de redacción. | `Reservado para release legal/cumplimiento` |

### Categoría 15: Cumplimiento Legal y de Licencias

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-15-001 | N/A | Atribución OSS | Rastrear obligaciones de licencia de terceros y avisos de redistribución. | `Cargo.toml`, docs de release |
| R-15-002 | N/A | Conciencia sobre Licencias de Códecs | Rastrear restricciones legales de códecs según jurisdicción de distribución. | `Checklist legal de release` |
| R-15-003 | N/A | Hooks de Términos y EULA (Reservado) | Reservar slots de requisitos para política de distribución comercial. | `Reservado para productización` |

### Categoría 20: Calidad de Software (QA y Testing)

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-20-001 | N/A | Cobertura Unit e Integración | Mantener línea base de unit/integration exitosa con evidencia CI repetible. | `cargo test`, `tests/`, tests en `crates/lap-core/src/*` |
| R-20-002 | N/A | Guardrails de Property/Fuzz | Pruebas basadas en propiedades y validación defensiva estilo fuzz para entradas hostiles. | `proptest` en áreas de sanitizer/security |
| R-20-003 | N/A | Líneas Base de Rendimiento | Benchmarking de build/test por paquete para detección de regresiones. | `cargo build -p`, benchmarks de release |

### Categoría 25: Documentación y Gobernanza

| ID | ID Heredado | Nombre | Descripción Técnica | Ancla de Implementación |
|---|---|---|---|---|
| R-25-001 | N/A | Disciplina ADR | Los cambios de arquitectura deben rastrearse con ciclo de vida de estado ADR. | `docs/adr/*` |
| R-25-002 | N/A | Integridad Docs-as-Code | Índice de documentación y docs canónicas actualizadas en el mismo conjunto de cambios que el código. | `docs/README.md`, `CHECKLIST.md` |
| R-25-003 | N/A | Gobernanza de Contribución | El flujo de contribución, barras de calidad y puertas de release son explícitos. | `CONTRIBUTING.md`, `CHECKLIST.md` |
| R-25-004 | N/A | Estándar de Documentación Asistida por IA | Toda documentación generada o asistida por IA debe usar la plantilla canónica de prompt estilo Google. La salida de IA no conforme no debe commitearse sin revisión humana contra la plantilla. | `docs/guides/ai_master_prompt_google_style.md`, `CONTRIBUTING.md` |
| R-25-005 | N/A | Aplicación Local de Pre-commit | Un hook de pre-commit de Git debe ejecutar `scripts/traceability_lint.sh` y bloquear el commit si el lint falla. Ningún entorno de contribuidor es conforme sin este hook instalado. | `.git/hooks/pre-commit`, `scripts/traceability_lint.sh` |

## Crosswalk Heredado-a-Nuevo

| Heredado | Nuevo | Notas |
|---|---|---|
| R-03 | R-02-008 | Sanitización de Nombre de Archivo |
| R-04 | R-01-003 | |
| R-05 | R-06-004 | Backup con Checksums |
| R-07 | R-09-009 | Planificador de Distribución (bucketing) |
| R-10 | R-08-001 | ⚠ Nota de numeración: este R-10 = Límite de Volumen (reqs v2.0); SDD-doc R-10 = Transcodificación de Audio -> R-08-002 |
| R-11 | R-02-001 | ⚠ Nota de numeración: este R-11 = Validador FAT32 (reqs v2.0); SDD-doc R-11 = Eliminación ID3 -> R-08-006 |
| R-12 | R-02-004 | ⚠ Nota de numeración: este R-12 = Descubrimiento Recursivo (reqs v2.0); SDD-doc R-12 = Dry Run -> R-01-004 |
| R-13 | R-02-009 | S.M.A.R.T. Lite (implementado; evidencia de integración adversarial pendiente) |
| R-14 | R-01-005 | Logging Estructurado (Fase 2, en progreso) |
| R-15 | R-08-004 | |
| R-T5 | R-09-010 | Verificación Final + Expulsión Segura |
| R-16 | R-09-007 | Checkpoint (atómico) |
| R-17 | R-09-008 | Recuperación (granular) |
| R-18 | R-02-003, R-02-005 | Lock de concurrencia + prueba pre-flight Fase 2 |
| R-19 | R-08-005 | Detección DRM y cuarentena (Fase 2) |
| R-20 | R-02-002 | Prueba de Dirty Bit |
| R-21 | R-08-003 | Extracción de Metadatos (heredado), R-02-006 ENOSPC (Fase 2) |
| R-22 | R-02-007 | Detección de fraude NAND (Fase 2) |
| R-23/26 | R-09-005 | Sync SHA256 |
| R-25/26 | R-09-006 | Cuarentena Segura |
| R-27 | R-08-002 | Normalización de Loudness |
| R-30 | R-09-002 | Jaula Pre-Ingesta |
| R-31 | R-09-001 | Refactor In-Situ |
| R-32 | R-09-003 | Guardia de Topología |
| R-33 | R-09-004 | Refactor In-Place |
| R-34 | R-05-001 | Contención de Rutas |
| R-35 | R-05-002 | Sanitizador de Shell |
| R-36 | R-05-003 | Sandbox de Metadatos |

## Matriz de Referencia de Auditoría

| Categoría | Estándar Relacionado | Intención de Auditoría |
|---|---|---|
| `02` / `09` | ISO/IEC 27001 | Validar controles de disponibilidad e integridad para operaciones en medios removibles. |
| `05` / `06` | NIST SP 800-53 | Validar controles de endurecimiento y postura de mitigación de riesgos. |
| `08` | EBU / AES | Validar compatibilidad técnica y restricciones de calidad multimedia. |
| `10` | GDPR / ISO 27701 | Validar privacidad desde el diseño y controles de manejo de datos personales. |
| `20` | ISO/IEC 25010 | Validar características de calidad de software y evidencia de pruebas. |

## Registro de Ciclo de Vida de Requisitos

| ID | Estado | Evidencia de Verificación / Nota de Auditoría |
|---|---|---|
| R-01-001 | IMPLEMENTED | Estructura de workspace establecida; auditable por límites de crates. |
| R-01-002 | VERIFIED | `docs/testing/integration_tests.md` -> `test_07_ipc_event_serialization_contract`. |
| R-01-003 | IMPLEMENTED | Existe contrato CLI; todavía sin evidencia de integración dedicada registrada. |
| R-01-004 | IMPLEMENTED | Existe rama `--dry-run` en `crates/lap-bin-provision/src/orchestrator.rs`; evidencia de integración pendiente. |
| R-01-005 | VERIFIED | `docs/testing/integration_tests.md` -> `test_16_session_log_is_created_with_json_entries`. |
| R-01-006 | VERIFIED | `docs/testing/integration_tests.md` -> `test_22_json_scan_unsupported_feature_returns_typed_fatal_error` (dispatch + error tipado en entrypoint). |
| R-01-007 | VERIFIED | `docs/testing/integration_tests.md` -> `test_21_json_ingest_emits_only_machine_readable_events` (contrato reportería JSON desacoplada). |
| R-02-001 | IMPLEMENTED | Existe validación de hardware; evidencia de integración específica FAT32 aún no registrada. |
| R-02-002 | VERIFIED | `docs/testing/integration_tests.md` -> `test_09_read_only_filesystem_maps_to_typed_error`. |
| R-02-003 | IMPLEMENTED | Existe lock en código; evidencia de integración de concurrencia pendiente. |
| R-02-004 | VERIFIED | `docs/testing/integration_tests.md` -> `test_02_real_audio_discovery`. |
| R-02-005 | VERIFIED | `docs/testing/integration_tests.md` -> `test_14_preflight_rw_probe_fails_fast_on_read_only_target`. |
| R-02-006 | VERIFIED | `docs/testing/integration_tests.md` -> `test_15_checkpoint_enospc_maps_to_storage_full`. |
| R-02-007 | VERIFIED | `docs/testing/integration_tests.md` -> `test_10_hardware_fraud_detected_after_five_hash_mismatches`. |
| R-02-008 | VERIFIED | `docs/testing/integration_tests.md` -> `test_01_real_sanitization_and_distribution`. |
| R-02-009 | IMPLEMENTED | Ancla de implementación en `crates/lap-core/src/hardware.rs` (`assert_hardware_health`) con invocación pre-I/O en `crates/lap-bin-provision/src/orchestrator.rs` (`provision_usb`, `resume_provisioning`). Evidencia de integración adversarial dedicada pendiente para subir a `VERIFIED`. |
| R-02-010 | IMPLEMENTED | Política de flush USB consolidado en fronteras de volumen/transacción; verificación cuantitativa instrumentada en `scripts/telemetry_r02_010_io_wear.sh`.
 Evidencia real (2026-03-25): corrida sobre biblioteca real (`Found 1692 audio files`) con `strace -c -e trace=fsync` reportó `8913 fsync`, ratio `8913/1692 = 5.267` (> 0.1), por lo que el requisito permanece en `IMPLEMENTED` y no puede subir a `VERIFIED`.
 Criterios de cierre: (1) sin `sync_all()` por archivo en ruta de provisión, (2) reducción >=20% de p95 de latencia en corrida de 500+ archivos vs baseline legacy, (3) ratio de `sync_all` por archivo <= 0.1 en ejecución nominal. |
| R-05-001 | VERIFIED | `docs/testing/integration_tests.md` -> `test_11_path_traversal_is_rejected`. |
| R-05-002 | VERIFIED | `docs/testing/integration_tests.md` -> `test_12_shell_injection_filename_is_rejected`. |
| R-05-003 | VERIFIED | `docs/testing/integration_tests.md` -> `test_13_metadata_bomb_is_rejected`. |
| R-06-001 | VERIFIED | `docs/testing/integration_tests.md` -> `test_05_sync_diff_ignores_existing_hashes`. |
| R-06-002 | VERIFIED | `docs/testing/integration_tests.md` -> `test_19_verify_file_integrity_detects_post_write_corruption`. |
| R-06-003 | PROPOSED | Reservado para versión futura. |
| R-06-004 | VERIFIED | `docs/testing/integration_tests.md` -> `test_04_end_to_end_backup_integration`. |
| R-08-001 | VERIFIED | `docs/testing/integration_tests.md` -> `test_01_real_sanitization_and_distribution`. |
| R-08-002 | IMPLEMENTED | Existe pipeline de normalización; no hay evidencia de integración directa registrada. |
| R-08-003 | IMPLEMENTED | Existe extracción de metadatos; no hay evidencia de integración directa registrada. |
| R-08-004 | IMPLEMENTED | Existe feedback de UI; no hay evidencia de integración dedicada registrada. |
| R-08-005 | VERIFIED | `docs/testing/integration_tests.md` -> `test_08_m4p_is_reported_as_drm_protected`. |
| R-08-006 | VERIFIED | `docs/testing/integration_tests.md` -> `test_23_in_place_e2e_applies_fast_and_slow_paths`. |
| R-09-001 | IMPLEMENTED | Existe workflow orquestado; evidencia de integración pendiente. |
| R-09-002 | IMPLEMENTED | Guardia de ruta canónica implementada; evidencia de integración pendiente. |
| R-09-003 | IMPLEMENTED | Guardia de topología implementada; evidencia de integración pendiente. |
| R-09-004 | VERIFIED | `docs/testing/integration_tests.md` -> `test_23_in_place_e2e_applies_fast_and_slow_paths`. |
| R-09-005 | VERIFIED | `docs/testing/integration_tests.md` -> `test_05_sync_diff_ignores_existing_hashes`. |
| R-09-006 | VERIFIED | `docs/testing/integration_tests.md` -> `test_06_orphan_isolation_to_quarantine`. |
| R-09-007 | VERIFIED | `docs/testing/integration_tests.md` -> `test_03_real_checkpoint_tracking`. |
| R-09-008 | VERIFIED | `docs/testing/integration_tests.md` -> `test_17_execute_recovery_restores_only_invalid_entries`. |
| R-09-009 | VERIFIED | `docs/testing/integration_tests.md` -> `test_01_real_sanitization_and_distribution`. |
| R-09-010 | IMPLEMENTED | `pre_eject_verification` tiene evidencia en `test_18_pre_eject_verification_accepts_valid_topology_and_hashes` y `test_20_root_topology_sweep_prevents_pre_eject_false_positives`, pero `safe_eject` todavía no tiene evidencia de sistema; el requisito completo permanece en IMPLEMENTED. |
| R-09-011 | VERIFIED | `docs/testing/integration_tests.md` -> `test_20_root_topology_sweep_prevents_pre_eject_false_positives`. |
| R-10-001 | IMPLEMENTED | Gobernado por política de revisión; no orientado a pruebas. |
| R-10-002 | IMPLEMENTED | Existe ciclo de vida operativo; gobernado por revisión. |
| R-10-003 | PROPOSED | Reservado para release de cumplimiento. |
| R-15-001 | IMPLEMENTED | Impulsado por checklist de gobernanza/legal. |
| R-15-002 | IMPLEMENTED | Impulsado por checklist de gobernanza/legal. |
| R-15-003 | PROPOSED | Reservado para productización. |
| R-20-001 | VERIFIED | Línea base `docs/testing/integration_tests.md` 81/81 exitosas. |
| R-20-002 | IMPLEMENTED | Existe evidencia `proptest` en `docs/testing/pbt_and_e2e_test_plan.md`; la puerta de integración aún no aplica. |
| R-20-003 | PROPOSED | Disciplina de benchmark reservada para endurecimiento de releases futuras. |
| R-25-001 | IMPLEMENTED | Disciplina ADR activa en `docs/adr/`. |
| R-25-002 | IMPLEMENTED | Flujo docs-as-code y checklist activos. |
| R-25-003 | IMPLEMENTED | Gobernanza de contribución/release documentada. |
| R-25-004 | IMPLEMENTED | Estándar de docs IA canonizado en `docs/guides/ai_master_prompt_google_style.md`; referenciado desde `CONTRIBUTING.md`. |
| R-25-005 | IMPLEMENTED | Hook pre-commit instalado en `.git/hooks/pre-commit`; invoca `scripts/traceability_lint.sh`. |

## Estado de Integración por Fase

**v0.3.0 (Actual)**: R-01 a R-15, R-23-26, R-27, R-30-36 (incl. R-T5). Provisionamiento core, seguridad, gobernanza y expulsión segura.

**Fase 2 (v0.3.x, en desarrollo)**: R-16-22 (Resiliencia, Concurrencia, Casos Borde). Enfoque en tolerancia a fallos, seguridad de concurrencia, manejo DRM y detección de fraude de hardware.
  - R-16, R-17: Checkpoint y Recuperación (implementado, ver `docs/architecture/R16_R17_checkpoint_and_recovery.md`)
  - R-18-22: Casos borde de Fase 2 (especificados en `docs/spec/sdd_edge_cases_phase2.md`)
  - R-13 (S.M.A.R.T. Lite): planificado, aún no implementado
  - R-14 (Logging Estructurado): implementado, evidencia de integración pendiente

> **Nota de numeración del crosswalk:** Los IDs heredados R-10, R-11, R-12 aparecen en dos sistemas de numeración independientes. Las notas al pie de la tabla crosswalk aclaran a cuál versión corresponde cada entrada. Al resolver una referencia, verifica siempre el documento fuente (reqs v2.0 vs. `docs/archive/spec_driven_development_legacy_numbering_reference.md`).

## Trabajo Residual para v0.3.1

1. Retirar la ruta de ejecución heredada en `src/main.rs` en favor de binarios de workspace (retiro controlado):
  - Fase A: declarar `src/` raíz como legado en documentación principal (`README.md`) y runbooks.
  - Fase B: eliminar referencias operativas activas a `src/main.rs` en guías no archivadas.
  - Fase C: validar que CI/release no dependen de `src/` raíz (solo `crates/*`).
  - Fase D: mover o archivar definitivamente `src/` raíz cuando no existan consumidores.
  - Estado 2026-03-24: Fase A completada; Fase B completada; Fase C completada; Fase D completada.
  - Evidencia Fase C: `cargo metadata --no-deps --format-version 1` reporta solo miembros `crates/*`; `cargo test --workspace --quiet` pasa en verde; `Makefile` y `CHECKLIST.md` actualizados a flujo por crates y baseline vigente.
  - Evidencia Fase D: módulos legacy de `src/*.rs` retirados del árbol activo y respaldados en `backups/docs_archive_20260324_194040.tar.gz`; el directorio `src/` raíz fue eliminado.
2. Mover pruebas de integración a suites con alcance por crate.
3. Expandir la aplicación de contención de rutas R-05 a todos los bordes restantes de escritura/movimiento USB.
4. Completar la implementación de requisitos de Fase 2 (R-18-22): endurecimiento de concurrencia, filtrado DRM, manejo ENOSPC, detección de fraude NAND.
5. Generar evidencia comparativa de rendimiento/desgaste para R-02-010 (telemetría de flush y latencia por lote).
