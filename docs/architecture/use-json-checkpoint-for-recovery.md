# Architecture Note: Use Atomic JSON Checkpoint for Recovery

## Status
Accepted

## Context
El pipeline puede ser interrumpido por SIGKILL, desconexion USB o corte de energia. Se requiere persistencia de progreso por archivo para permitir `--resume` sin reprocesar todo ni duplicar escrituras.

Opciones evaluadas:
- SQLite embebido.
- Archivo JSON atomico.

## Decision
Se usa archivo JSON versionado (`.provisioning_checkpoint`) con estructura `BTreeMap<usize, FileCheckpoint>` y escritura atomica:
1. escribir `checkpoint.tmp`
2. `sync_all()`
3. `rename()` atomico a checkpoint final

## Consequences
### Positive
- Estado legible y auditable sin tooling extra.
- Recuperacion granular por indice y estado (`InProgress`, `Completed`, `Failed`).
- Mitiga corrupcion por escritura parcial del checkpoint.

### Negative
- Sin transacciones SQL complejas.
- Costo de serializacion completa por escritura.

### Neutral
- Requiere cuidado en compatibilidad de version del schema JSON.
