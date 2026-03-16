# Architecture Note: Force Physical Write Through Normalization Pipeline

## Status
Accepted

## Context
Copiar bytes crudos (`fs::copy`) desde origen a USB no garantiza compatibilidad firmware legacy. Formatos no MP3, metadatos pesados o parametros incompatibles provocan fallos de reproduccion.

## Decision
Toda escritura fisica de archivos de audio a USB se realiza a traves de `normalizer::normalize_audio(...)` en el orquestador (`main.rs`) tanto en provision inicial como en recovery.

`distribution.rs` queda como planificador puro en memoria (sin I/O).

## Consequences
### Positive
- Convergencia de formato de salida (MP3 compatible).
- El recovery no repite errores de formato de una copia cruda.
- Punto unico para checksum final real post-normalizacion.

### Negative
- Dependencia operativa de ffmpeg/ffprobe.
- Mayor costo de CPU frente a copia directa.

### Neutral
- Observabilidad mejorada via logs y progress bar en el paso de normalizacion.
