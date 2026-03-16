# Architecture Note: Require sync Before Unmount/Power-Off on Linux

## Status
Accepted

## Context
En FAT32, desmontar o cortar energia sin flush explicito puede dejar metadata en page cache y producir corrupcion silenciosa.

## Decision
`verification::safe_eject` en Linux ejecuta secuencia obligatoria:
1. `sync`
2. `umount <mount_point>`
3. `udisksctl power-off -b <device_path>`

## Consequences
### Positive
- Reduce riesgo de FAT inconsistente por cache pendiente.
- Flujo de expulsado reproducible y auditable.

### Negative
- Puede fallar `umount` si el volumen esta busy.
- Dependencia de utilidades del sistema (`umount`, `udisksctl`).

### Neutral
- En plataformas no Linux se degrada a aviso de expulsado manual.
