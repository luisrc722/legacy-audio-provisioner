# Architectural Decisions

Documento de decisiones técnicas de alto nivel para la Fase 2 del motor.
Complementa los ADRs en `docs/architecture/` con foco operativo en Sync, integridad y tolerancia a fallos.

## AD-01: USB como Fuente de Verdad en modo `--sync`

- Decisión: usar diff SHA256 entre `audio-source` y USB para procesar solo novedades.
- Estado persistente: `.provisioning_checkpoint` espejado también en raíz USB.
- Justificación:
  - evita reprocesamiento masivo,
  - preserva continuidad de índices globales `N+1`,
  - permite operación incremental sobre USB ya pobladas.
- Consecuencia:
  - cómputo de hash adicional durante diff,
  - menor CPU total al evitar transcodificar duplicados.

## AD-02: Continuidad de índices y topología FAT32

- Decisión: mantener numeración global continua y ocupación por volumen (`VOL_XX`, máx 50 archivos).
- Regla:
  - calcular `max_existing_index` en USB/checkpoint,
  - comenzar nuevos archivos desde `max + 1`,
  - rellenar último volumen parcial antes de abrir uno nuevo.
- Justificación:
  - previene colisiones de nombres,
  - mantiene orden reproducible para firmwares que leen por orden FAT.

## AD-03: Transaccionalidad y seguridad de hardware (Zero-Trust)

- Decisión:
  - lock físico `.lap_provisioning.lock` para exclusión mutua,
  - dirty-bit test (`assert_rw_filesystem`) antes de I/O,
  - checkpoint atómico POSIX (`tmp -> sync_all -> rename + dir sync`).
- Justificación:
  - evitar corrupción por concurrencia y dispositivos en modo read-only,
  - garantizar recuperabilidad tras cortes abruptos.

## AD-04: Detección de fraude NAND por anomalía criptográfica

- Decisión: abortar proceso con `HARDWARE_FRAUD_DETECTED` tras 5 mismatches SHA256 consecutivos en validación final.
- Justificación:
  - dispositivos con capacidad falsificada sobreescriben bloques y aparentan éxito de escritura,
  - la señal de mismatches consecutivos es un indicador operativo fuerte de spoofing.

## AD-05: Normalización destructiva para compatibilidad legacy

- Decisión:
  - passthrough de MP3 seguro,
  - transcodificación forzada a MP3 CBR 128k cuando no cumple perfil,
  - limpieza estricta de streams/metadatos (`-map 0:a:0`, `-map_metadata -1`).
- Justificación:
  - firmware legacy falla con VBR, tags complejos y carátulas embebidas.

## AD-06: Sanitización determinista con preservación de extensión

- Decisión: sanitizar a ASCII y limitar a 32 caracteres garantizando extensión final (`.mp3`) y prefijo secuencial.
- Justificación:
  - evita archivos ilegibles por firmware,
  - elimina el bug de truncamiento que rompía la extensión.

## AD-07: Política no destructiva para `untracked` (Cuarentena)

- Decisión:
  - no borrar por defecto archivos huérfanos,
  - aplicar aislamiento `backup-first` hacia `.legacy_quarantine/<session>/`.
- Justificación:
  - minimiza riesgo contractual de pérdida de datos del cliente,
  - deja USB operativa para estéreo sin descartar evidencia ni contenido.
- Flujo:
  1. Backup en host.
  2. Move a cuarentena oculta en USB.
  3. Sync de directorios.

## AD-08: Contrato de errores tipados e IPC para frontend

- Decisión:
  - centralizar fallos de dominio en `ProvisioningError`,
  - mapear eventos operativos a JSON IPC (`PROGRESS`, `WARNING`, `FATAL_ERROR`, `SUCCESS`).
- Justificación:
  - frontera estable backend/frontend,
  - mayor observabilidad y automatización de remediación en UI.

## Hardware Compatibility Matrix

| Factor | Política LAP | Resultado esperado |
| --- | --- | --- |
| FS destino | `vfat`/FAT32 obligatorio | Evita escrituras sobre discos no legacy |
| Dispositivo | Removible por kernel | Reduce riesgo de daño al host |
| Nombres | ASCII, <= 32 chars | Compatibilidad de parser en firmware |
| Audio | MP3 CBR 128-192kbps | Reproducción estable en estéreo 2005 |
| Volúmenes | `VOL_XX`, <=50 archivos | Evita overflow de buffers legacy |

## Mermaid: Lógica de Sync Incremental

```mermaid
flowchart TD
    A[Read source files] --> B[Read USB checkpoint]
    B --> C[Scan USB hashes]
    C --> D{Hash exists in USB?}
    D -- Yes --> E[Skip]
    D -- No --> F[Assign next global index]
    F --> G[Plan VOL_XX allocation]
    G --> H[Normalize + Write]
    H --> I[Checkpoint update]
    I --> J[Mirror checkpoint to USB]
```

## QA Baseline (Fase 2)

Estado actual verificado por suite automatizada:

- Unit: 41
- Integration: 11
- Doc tests: 2
- Total: 54/54 passing

Cobertura de fallos críticos incluida:

- `DRM_PROTECTED`
- `FILESYSTEM_READ_ONLY`
- `ENOSPC_ERROR`
- `HARDWARE_FRAUD_DETECTED`
