# Nota de Arquitectura: Requerir sync Antes de Unmount/Power-Off en Linux

## Estado
Aceptado

## Contexto
En FAT32, desmontar o cortar energía sin flush explícito puede dejar metadatos en page cache y producir corrupción silenciosa.

## Decisión
`verification::safe_eject` en Linux ejecuta secuencia obligatoria:
1. `sync`
2. `umount <mount_point>`
3. `udisksctl power-off -b <device_path>`

## Consecuencias
### Positivas
- Reduce riesgo de FAT inconsistente por cache pendiente.
- Flujo de expulsado reproducible y auditable.

### Negativas
- Puede fallar `umount` si el volumen está ocupado.
- Dependencia de utilidades del sistema (`umount`, `udisksctl`).

### Neutrales
- En plataformas no Linux se degrada a aviso de expulsado manual.
