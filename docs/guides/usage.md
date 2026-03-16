# Guía de Uso - Legacy Audio Provisioner

## Escenario de Ejemplo

Supongamos que tienes:
- Una colección de MP3 en `~/MiMusica/`
- Una USB montada en `/media/usuario/DISCO_USB`
- Un estéreo antiguo que solo acepta FAT32 con máximo 50 archivos por carpeta

## Paso 1: Montar la USB (Manual)

```bash
# En Linux/macOS
mount /dev/sdb1 /media/usuario/DISCO_USB

# Verificar que está montada
df -h | grep DISCO_USB
```

## Paso 2: Ejecutar Provisioner (Primer intento - Dry Run)

Siempre es bueno hacer una ejecución en simulación primero:

```bash
# Compilar si aún no está hecho
cargo build --release

# Ejecutar en modo simulación
./target/release/legacy-audio-provisioner \
  --usb-mount /media/usuario/DISCO_USB \
  --audio-source ~/MiMusica \
  --dry-run \
  --verbose
```

### Esperado:

```
=== Legacy Audio Provisioner ===
Version 0.1.0 | Spec-Driven Development

=== Starting USB Provisioning ===
[DRY RUN] No actual changes will be made

📋 Step 1: Validating USB device...
✓ USB device validated: /media/usuario/DISCO_USB

� Step 2: Scanning audio files (Secure Mode)...
✓ Found 127 audio files

💾 Step 3: Creating backup and validating disk space...
[DRY RUN] Skipping backup creation

🧹 Step 4: Sanitizing filenames & Initializing Checkpoint...
✓ Planned 3 volume(s)

=== Provisioning Complete ===
```

## Paso 3: Ejecutar en Real (Sin --dry-run)

Una vez que el dry-run se vea bien:

```bash
./target/release/legacy-audio-provisioner \
  --usb-mount /media/usuario/DISCO_USB \
  --audio-source ~/MiMusica \
  --verbose
```

## Ejemplos Avanzados

### Caso 1: USB muy grande (> 64 GB)

```bash
./target/release/legacy-audio-provisioner \
  --usb-mount /media/usuario/DISCO_GRANDE \
  --audio-source ~/MiMusica \
  --verbose
# Mensaje esperado:
# ⚠️  Device size: 128.50 GB (requires confirmation for safety)
```

### Caso 2: Debugging con logs detallados

```bash
RUST_LOG=trace ./target/release/legacy-audio-provisioner \
  --usb-mount /media/usuario/DISCO_USB \
  --audio-source ~/MiMusica \
  -vvv
```

### Caso 3: Listar dispositivos detectados

```bash
./target/release/legacy-audio-provisioner --list-devices
```

### Caso 4: Reanudar una provisión interrumpida

Si el proceso fue interrumpido (corte de luz, desconexión USB), el checkpoint atómico preservó el estado exacto. Para reanudar:

```bash
./target/release/legacy-audio-provisioner \
  --usb-mount /media/usuario/DISCO_USB \
  --resume ~/usb_backup_20260315_1430
```

El recovery compara los SHA256 reales de la USB contra el checkpoint y solo recopia los archivos faltantes o corruptos. Los archivos ya copiados correctamente **no se tocan**.

## Estructura Resultante en USB

Después de ejecutar, tu USB se verá así:

```
DISCO_USB/
├── VOL_01/
│   ├── 001_Cancion_1.mp3
│   ├── 002_Cancion_2.mp3
│   ├── 003_Cancion_3.mp3
│   └── ... (hasta 50 archivos)
├── VOL_02/
│   ├── 001_Cancion_51.mp3
│   ├── 002_Cancion_52.mp3
│   └── ... (hasta 50 archivos)
└── VOL_03/
    └── ... (archivos restantes)
```

**Importante**: Los números (001_, 002_, etc.) ayudan al firmware del estéreo a:
1. Mantener el orden de reproducción
2. Evitar problemas de punteros en memoria
3. Facilitar la navegación secuencial en la FAT

## Backup Automático

Antes de copiar cualquier archivo a la USB, el programa crea una copia local:

```
~/usb_backup_20260315_1430/
├── 001_Cancin_1.mp3
├── 002_song.mp3
├── ...
└── .provisioning_checkpoint   ← estado atómico de la sesión
```

El directorio de backup se crea en `$HOME` por defecto. El checkpoint permite reanudar con `--resume` si ocurre una interrupción.

## Verificación Post-Provisioning

Después de provisionar, el estéreo debería:
1. ✓ Montar la USB sin errores
2. ✓ Reconocer 3 carpetas (VOL_01, VOL_02, VOL_03)
3. ✓ Reproducir archivos en orden numérico
4. ✓ No tener problemas de memoria/buffer

## Troubleshooting

### "Device is not removable"

```
Error: Device is not removable. Safety check failed.
```

**Solución**: Estás apuntando a un disco duro interno, no a USB.
Verifica con `lsblk` o `df -h`.

### "Invalid filesystem: ntfs"

```
Error: Invalid filesystem: ntfs. Only FAT32 is supported.
```

**Solución**: Formatea la USB a FAT32:
```bash
# Advertencia: ESTO BORRA DATOS
sudo mkfs.vfat -F 32 /dev/sdb1
sudo mount /dev/sdb1 /media/usuario/DISCO_USB
```

### "Mount point does not exist"

```
Error: Mount point does not exist: /media/usuario/DISCO_USB
```

**Solución**: Monta la USB primero:
```bash
sudo mkdir -p /media/usuario/DISCO_USB
sudo mount /dev/sdb1 /media/usuario/DISCO_USB
```

### Espacio insuficiente

```
Error: Not enough disk space for backup
```

**Solución**: Libera espacio en tu home directory:
```bash
# Ver tamaño de backups anteriores
du -sh ~/usb_backup*

# Eliminar backups antiguos si es necesario
rm -rf ~/usb_backup_20260101*
```

## Performance

### Tiempos Esperados

- Escaneo: ~1-2 seg (1000 archivos)
- Sanitización: ~0.1 seg
- Distribución: ~0.05 seg
- Verificación: ~2-5 seg (depende de tamaño)

**Total**: ~5-10 segundos para 1000 archivos

### Tamaño de Respaldo

- 100 MP3 de 5MB = ~500MB → backup ~500MB también
- Úsalo con moderación en discos pequeños

## Especificaciones Técnicas

### Límites Soportados (por especificación legado)

| Parámetro | Límite |
|-----------|--------|
| Profundidad de directorios | 2 niveles |
| Archivos por carpeta | 50 máximo |
| Longitud de nombre | 32 caracteres |
| Tamaño de clúster FAT32 | 32 KB |
| Particiones soportadas | MBR only |
| Encoding de nombres | ASCII/ISO-8859-1 |
| Regex de limpieza | `[^a-zA-Z0-9\.\-\_]` |
| Regex compilado vía | `std::sync::OnceLock` |
| Protección de extensión | Matemática (stem truncado, ext preservada) |

### Formatos de Audio Soportados

MP3, FLAC, WAV, OGG, M4A, ALAC, AAC, WMA, OPUS, AIFF

Los metadatos AppleDouble (`._archivo`) y carpetas ocultas (`.Trash`, `.Spotlight`) son filtrados **antes** del escaneo, previniendo panics en estéreos legacy.

## Ejemplo Completo Real

```bash
# 1. Verificar dispositivo
$ lsblk
sdb           8:16   1  31.2G  0 disk
└─sdb1        8:17   1  31.2G  0 part

# 2. Montar (si no está automático)
$ sudo mount /dev/sdb1 /media/user/DISK
$ df -h | grep DISK
/dev/sdb1      31G  2.1G   29G   7% /media/user/DISK

# 3. Compilar
$ cd ~/Projects/legacy-audio-provisioner
$ cargo build --release

# 4. Dry-run
$ ./target/release/legacy-audio-provisioner \
    --usb-mount /media/user/DISK \
    --audio-source ~/Music \
    --dry-run \
    -v

# 5. Ejecutar en real
$ ./target/release/legacy-audio-provisioner \
    --usb-mount /media/user/DISK \
    --audio-source ~/Music

# 6. Verificar
$ ls -la /media/user/DISK/
total 20
drwxr-xr-x  3 user user  4096 Mar  6 14:30 .
drwxr-xr-x  7 user user  4096 Mar  6 14:29 ..
drwxr-xr-x  2 user user  4096 Mar  6 14:30 VOL_01
drwxr-xr-x  2 user user  4096 Mar  6 14:30 VOL_02

# 7. Ejectar USB de forma segura
$ eject /dev/sdb1

# 8. O simplemente:
$ sudo umount /dev/sdb1
```

**¡Listo!** Tu USB está lista para el estéreo antiguo.
