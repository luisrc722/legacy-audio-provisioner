# ADR 0007: Validación de Ruta Canónica y Prevención de Circularidad

- **Estado:** Aceptado
- **Date:** 2026-03-16
- **Requirement:** R-30

## 1. Contexto

La CLI acepta `--usb-mount` y `--audio-source` como rutas independientes. Existe alto riesgo de circularidad si un usuario apunta por error ambos argumentos al mismo dispositivo, o si `audio-source` es un subdirectorio dentro de `usb-mount` (por ejemplo, la carpeta de cuarentena `.legacy_quarantine` o un directorio `VOL_XX` ya creado).

Esto causaría que el motor lea archivos que acaba de escribir, sobrescriba originales durante la normalización o entre en un bucle infinito de escaneo consumiendo disco hasta llenar el dispositivo.

## 2. Decisión

Implementar una capa estricta de validación de rutas canónicas (`validate_canonical_paths`) invocada **antes de cualquier I/O** en el comando `provision`:

1. **Resolución vía `canonicalize()`:** Ambas rutas se resuelven a su identidad real en filesystem, eliminando symlinks, `./`, `../` y alias de hardware antes de comparar.
2. **Bloqueo por igualdad:** Si `usb_canonical == source_canonical`, abortar con `ProvisioningError::InvalidConfig`.
3. **Bloqueo por anidamiento:** Si `source_canonical.starts_with(usb_canonical)`, abortar con `ProvisioningError::InvalidConfig`. Esto evita tratar la propia salida del motor (`VOL_XX`, `.legacy_quarantine`) como nuevo audio de origen.
4. **Mapeo de errores:** Los fallos se exponen como `INVALID_CONFIG` en el stream JSON IPC y en el mensaje de error legible para humanos.

Esta validación intencionalmente **no** se aplica a `resume`, porque ese comando lee desde un directorio de backup del host (`$HOME/usb_backup_*`), una ruta estructuralmente independiente de `usb-mount`.

## 3. Consecuencias

**Positivas:**
- Protección absoluta contra corrupción de datos por procesamiento circular.
- Resolución determinista de rutas sin importar cómo escriba la ruta el usuario (relativa, absoluta, symlink).
- Mensaje de error claro y accionable en la entrada de la CLI, antes de tocar cualquier dispositivo.

**Negativas:**
- `canonicalize()` requiere que ambas rutas existan en disco al momento de la validación. Las rutas que aún no existen no pueden pre-validarse.

## 4. Relación con Otros ADRs

| ADR | Connection |
| :--- | :--- |
| ADR-0004 Quarantine Isolation | R-30 asegura que los directorios de cuarentena nunca se re-ingesten como origen |
| ADR-0005 Sync SHA256 | Las rutas canónicas garantizan que los hashes se comparen sobre el mismo dispositivo físico |
| ADR-0006 Docs-as-Code | Esta decisión se documenta aquí y se referencia en tech_spec.md |
