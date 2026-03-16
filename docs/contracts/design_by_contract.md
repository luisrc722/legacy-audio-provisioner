# Design by Contract (DbC)

## Scope
Contratos formales a nivel de módulo para garantizar la seguridad del hardware del host, la integridad transaccional de los datos y la recuperación determinista en hardware legacy (FAT32, firmware 32-bit).

## Global Invariants
- **Inmutabilidad del Origen:** El directorio `--audio-source` es estrictamente de solo lectura. Ningún archivo original debe ser mutado o eliminado bajo ninguna circunstancia.
- **Seguridad de Destino:** El destino `--usb-mount` debe corresponder obligatoriamente a un dispositivo de bloque físico marcado como removible por el kernel, formateado en FAT32.
- **Single Source of Truth:** El archivo JSON del Checkpoint es la única fuente de verdad autorizada para el estado de la provisión y la recuperación ante desastres.
- **Topología Legacy:** Ningún nombre de archivo en la USB excederá los 32 caracteres ASCII, y ningún directorio contendrá más de 50 archivos.

---

## Module Contracts

### `hardware`
#### `validate_device_path(target_mount_point)`
* **Preconditions:** `target_mount_point` debe existir como un path absoluto inyectado por la CLI.
* **Postconditions:** Retorna `DeviceInfo` con el bloque físico padre resuelto (ej. `sdb`, `mmcblk0`). Retorna `Err` si el `fs_type` no es FAT32 o si el bit `/sys/block/X/removable` es `0`. Levanta un flag de advertencia si la capacidad $> 64GB$.
* **Invariants:** No se debe asumir, sintetizar ni falsear la propiedad `is_removable`. La lectura es directa al kernel.

### `audio_discovery`
#### `discover_audio_files(root)`
* **Preconditions:** `root` existe, es accesible y es un directorio.
* **Postconditions:** Retorna un reporte estructurado de archivos cuyas extensiones pertenezcan a la lista blanca (`AUDIO_EXTENSIONS`). Se ignoran silenciosamente subárboles y dotfiles de sistema (`.*`, `System Volume Information`, `$RECYCLE.BIN`, `FOUND.*`).
* **Invariants:** La validación de ocultamiento (`is_hidden`) jamás se aplica al nodo raíz (`depth=0`).

### `sanitizer`
#### `sanitize_filename` + `add_sequential_prefix`
* **Preconditions:** La cadena de entrada puede contener UTF-8 arbitrario, emojis y extensiones largas.
* **Postconditions:** Retorna un `String` truncado matemáticamente donde la longitud total (incluyendo el prefijo numérico `001_` y la extensión `.mp3`) es $\le$ 32 bytes. Todos los caracteres son forzados a ASCII seguro.
* **Invariants:** La extensión del archivo destino es inmutablemente `.mp3`.

### `normalizer` (NUEVO)
#### `normalize_audio(source_path, dest_path)`
* **Preconditions:** `source_path` es un archivo de audio válido. `dest_path` reside dentro de un volumen estructurado `VOL_XX`. FFmpeg 4.2+ está disponible en el `$PATH`.
* **Postconditions:** `dest_path` se escribe en disco. El archivo resultante es estrictamente un contenedor MPEG Layer III (MP3), CBR (128k-192k), a 44.1/48kHz, y **carece por completo** de etiquetas ID3 e imágenes incrustadas (`-map_metadata -1`).
* **Invariants:** Fallar la transcodificación aborta la escritura local pero no corrompe el Checkpoint global.

### `distribution`
#### `plan_distribution(file_mappings)`
* **Preconditions:** `file_mappings` contiene rutas origen válidas y nombres destino ya sanitizados.
* **Postconditions:** Devuelve `Vec<VolumeSegment>` ordenado, donde ningún segmento (`files.len()`) excede el límite rígido de 50 archivos.
* **Invariants:** Este módulo es pura matemática de planificación. Tiene cero efectos secundarios (I/O física prohibida).

### `checkpoint`
#### `save_to_disk`
* **Preconditions:** El directorio de trabajo (`backup_dir`) tiene permisos de escritura y espacio disponible.
* **Postconditions:** El estado de `processed_files` se vuelca al disco.
* **Invariants:** La persistencia es estrictamente atómica POSIX: creación de `.tmp` $\rightarrow$ `sync_all()` $\rightarrow$ `rename()`. El archivo principal jamás queda en estado truncado.

### `recovery`
#### `execute_recovery(checkpoint_mgr)`
* **Preconditions:** El `CheckpointData` en disco es legible y su estado global es recuperable (no finalizado). El dispositivo USB está montado.
* **Postconditions:** Recalcula el `SHA256` físico de los archivos marcados como `Completed` en la USB. Purga inodos huérfanos (archivos de 0 bytes). Re-ejecuta el `normalizer` para las entradas `Failed` o `InProgress`.
* **Invariants:** Recuperación estrictamente granular e idempotente. Prohibido formatear o borrar el volumen completo.

### `verification`
#### `pre_eject_verification(usb_mount, checkpoint)`
* **Preconditions:** El ciclo de copia ha terminado. El USB sigue montado.
* **Postconditions:** Retorna un reporte de auditoría. Falla (`success = false`) si detecta anidamiento ilegal, carpetas con $>50$ archivos, nombres $>32$ bytes, o si los hashes SHA256 físicos difieren del Checkpoint.
* **Invariants:** Debe ejecutarse y aprobarse obligatoriamente antes de llamar a `checkpoint.finalize()`.

#### `safe_eject(device_path, mount_point)`
* **Preconditions:** El pipeline ETL y la verificación QA concluyeron con éxito.
* **Postconditions:** El dispositivo se desmonta y el puerto USB se apaga lógicamente.
* **Invariants:** Es obligatorio invocar la syscall `sync` para vaciar el *Page Cache* del kernel hacia la memoria flash antes del comando `umount`.

---

## Orchestrator Contract (`main.rs`)
**Flujo de Provisión:**
1. Validar hardware (`hardware`).
2. Descubrir audio en origen (`audio_discovery`).
3. Crear backup temporal de metadatos y verificar cuotas de disco.
4. Planificar nombres y volúmenes (`sanitizer` + `distribution`).
5. Normalizar I/O destructiva y escribir USB (`normalizer` + `checkpoint`).
6. Auditar invariantes finales (`verification`).
7. Cerrar transacción (`checkpoint.finalize()`).
8. Expulsar dispositivo físicamente (`safe_eject`).

**Flujo de Reanudación (`--resume`):**
1. Cargar Checkpoint atómico desde disco.
2. Validar que la sesión no haya sido finalizada lógicamente.
3. Ejecutar recuperación granular criptográfica (`recovery`).

## Error Contract
- Los fallos críticos o violaciones de invariantes abortan la ejecución inmediatamente retornando `anyhow::Result::Err` con contexto enriquecido.
- Ninguna operación se marca como `Completed` si su verificación criptográfica o de hardware falla.
