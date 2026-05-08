# Requisitos R-01 a R-17 - Diagramas Visuales

Este documento resume por que existen los IDs `R-01 ... R-17` y como se conectan con codigo, tests y operacion.

## 1) Mapa General De Requisitos

```mermaid
flowchart LR
    R01[R-01 Particionado/FAT32]
    R02[R-02 Estructura de directorios]
    R03[R-03 Sanitizacion de nombres]
    R04[R-04 Deteccion de hardware]
    R05[R-05 Backup + integridad]
    R06[R-06 Discovery + normalizacion]
    R07[R-07 Distribucion por volumen]
    R12[R-12 Dry-run]
    R14[R-14 Logging]
    R15[R-15 Feedback/progreso]
    R16[R-16 Checkpoint atomico]
    R17[R-17 Recovery granular]
    RT5[R-T5 Verificacion final + eject]

    R04 --> R05 --> R06 --> R07 --> R16 --> R17 --> RT5
    R03 --> R07
    R02 --> R07
    R12 --> R04
    R12 --> R05
    R14 --> R16
    R15 --> R06
    R01 --> R04
```

## 2) Flujo Operativo (Provision)

```mermaid
flowchart TD
    A[Inicio CLI] --> B[R-04 Validar USB removible FAT32]
    B --> C[R-06 Descubrir audio]
    C --> D[R-05 Crear backup + checksum]
    D --> E[R-03 Sanitizar nombres]
    E --> F[R-07 Planear VOL_XX <= 50]
    F --> G[R-16 Guardar progreso por archivo]
    G --> H[R-06 Normalizar y copiar]
    H --> I[R-T5 Verificar estructura e integridad]
    I --> J[Finalizar checkpoint]
    J --> K[Expulsión segura]

    H --> L{Fallo/interrupcion?}
    L -- Si --> M[R-17 Recovery con --resume]
    M --> G
    L -- No --> I
```

## 3) Trazabilidad Requisito -> Codigo

```mermaid
flowchart LR
    R03[R-03] --> S[crates/lap-core/src/sanitizer.rs]
    R04[R-04] --> H[crates/lap-core/src/hardware.rs]
    R05[R-05] --> B[crates/lap-core/src/backup.rs]
    R06[R-06] --> A[crates/lap-core/src/audio_discovery.rs]
    R06 --> N[crates/lap-core/src/normalizer.rs]
    R07[R-07] --> D[crates/lap-core/src/distribution.rs]
    R16[R-16] --> C[crates/lap-core/src/checkpoint.rs]
    R17[R-17] --> R[crates/lap-core/src/recovery.rs]
    RT5[R-T5] --> V[crates/lap-core/src/verification.rs]
    ORCH[Orquestacion] --> M[crates/lap-bin-provision/src/orchestrator.rs]
```

## 4) Trazabilidad Requisito -> Pruebas

```mermaid
flowchart LR
    R03[R-03 Sanitizacion] --> T1[test_01_real_sanitization_and_distribution]
    R06[R-06 Discovery] --> T2[test_02_real_audio_discovery]
    R16[R-16 Checkpoint] --> T3[test_03_real_checkpoint_tracking]
    R05[R-05 Backup] --> T4[test_04_end_to_end_backup_integration]
    SYS[Dependencias runtime] --> T0[test_00_system_dependencies]

    T1 --> IT[crates/lap-core/tests/integration_test.rs]
    T2 --> IT
    T3 --> IT
    T4 --> IT
    T0 --> IT
```

## 5) Por Que No Se Nombra Por Archivo

- Un requisito no equivale a un archivo.
- Un requisito cruza modulos, CLI, verificacion y tests.
- Los IDs `R-XX` permiten saber que garantia se toca aunque el codigo cambie de archivo.
- Si el proyecto crece, la trazabilidad se mantiene estable.

## 6) Lectura Rapida

- Si cambias compatibilidad de nombres: impacta `R-03`.
- Si cambias como se detecta USB: impacta `R-04`.
- Si cambias reanudacion tras fallo: impacta `R-16` y `R-17`.
- Si cambias validacion final antes de expulsar: impacta `R-T5`.

## 7) Patrón Industrial Mermaid (Docs-as-Code)

Markdown puro no renderiza diagramas por si solo. En la practica, el estandar en repositorios es usar bloques `mermaid` para documentacion viva en GitHub/GitLab/VS Code.

### 7.1 Flowchart (Pipeline)

```mermaid
graph TD
    A[Directorio Origen] -->|audio_discovery| B[Filtro AppleDouble]
    B --> C{Es MP3 seguro?}

    C -->|Si: CBR 128k-192k| D[Passthrough]
    C -->|No: FLAC/VBR| E[FFmpeg transcodificar]

    E -->|Limpiar metadatos| F[Sanitizer <= 32 chars]
    D --> F

    F --> G[(USB VOL_XX)]
    G --> H{Verificacion SHA256}
    H -->|Coincide| I[Checkpoint Completado]
    H -->|No coincide| J[Checkpoint Fallido]
```

### 7.2 State Diagram (Checkpoint)

```mermaid
stateDiagram-v2
    [*] --> InProgress : Inicia provision

    InProgress --> Completed : normalizacion/copia OK + SHA256 match
    InProgress --> Failed : error I/O o ffmpeg

    Failed --> InProgress : --resume
    Completed --> [*] : finalize()
```

### 7.3 Sequence Diagram (Expulsión Segura)

```mermaid
sequenceDiagram
    participant O as Orquestador (main)
    participant V as Verificador
    participant K as Kernel Linux
    participant U as USB

    O->>V: pre_eject_verification()
    V->>U: Lee estructura VOL_XX
    V->>O: Reporte OK
    O->>V: safe_eject()
    V->>K: sync
    Note over K,U: Volcar page cache a memoria flash
    V->>K: umount <mount_point>
    V->>K: udisksctl power-off -b <device>
    K->>U: Corta energia del puerto
```

### 7.4 Class Diagram (Planificacion en Memoria)

```mermaid
classDiagram
    class VolumeSegment {
        +String folder_name
        +usize volume_index
        +Vec~DistributedFile~ files
        +is_full() bool
    }

    class DistributedFile {
        +PathBuf source_path
        +String sanitized_name
    }

    VolumeSegment "1" *-- "0..50" DistributedFile : contiene
```

### 7.5 Recomendacion de Uso

- `Flowchart`: arquitectura de pipeline y decisiones de proceso.
- `State`: recovery/checkpoint y transiciones de error.
- `Sequence`: interaccion con OS y pasos de seguridad.
- `Class`: relaciones entre structs y limites en memoria.
