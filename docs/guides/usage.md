# Guía de Uso - Legacy Audio Provisioner

## Escenario de Ejemplo

Supongamos que tienes:
- Una colección de MP3 en `~/MiMusica/`
- Una USB montada en `/media/usuario/DISCO_USB`
- Un estéreo antiguo que solo acepta FAT32 con máximo 50 archivos por carpeta

## Playbook Rápido: USB Nueva (3 comandos)

Si la USB está vacía, se trata como destino nuevo. Este es el flujo recomendado:

```bash
# 1) Detectar y validar dispositivos
cargo run -p lap-bin-provision -- list

# 2) (Opcional) Reformatear a perfil legacy FAT32 32KB si no cumple
cargo run -p lap-bin-provision -- \
  format \
  --usb /media/usuario/DISCO_USB \
  --confirm-device /dev/sdb1

# 3) Provisionar desde cero
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica \
  --verbose
```

Notas:
- Si en el paso 2 la USB ya cumple perfil legacy, `format` termina en no-op seguro.
- En USB vacía, el backup-first es valido y puede resultar en no-op de contenido.
- El primer `provision` copia todo como contenido nuevo y deja estado operativo host-only en `~/.lap`.

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
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica \
  --dry-run \
  --verbose
```

### Esperado:

```
=== Legacy Audio Provisioner ===
Version 0.1.0 | Spec-Driven Development

=== Iniciando provisión USB ===
[DRY RUN] No se realizarán cambios reales

📋 Paso 1: Validando dispositivo USB...
✓ Dispositivo USB validado: /media/usuario/DISCO_USB

� Paso 2: Escaneando archivos de audio (Modo Seguro)...
✓ Se encontraron 127 archivos de audio

💾 Paso 3: Creando respaldo y validando espacio en disco...
[DRY RUN] Omitiendo creación de respaldo

🧹 Paso 4: Sanitizando nombres e inicializando checkpoint...
✓ Planned 3 volume(s)

=== Provisión completada ===
```

## Paso 3: Ejecutar en Real (Sin --dry-run)

Una vez que el dry-run se vea bien:

```bash
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica \
  --verbose
```

Nota de montaje al finalizar:
- Por defecto la USB queda montada al terminar `provision`.
- Si quieres expulsión automática segura al final, ejecuta con `LAP_SAFE_EJECT=1`.

```bash
LAP_SAFE_EJECT=1 cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica
```

## Ejemplos Avanzados

### Caso 1: USB muy grande (> 64 GB)

```bash
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_GRANDE \
  --source ~/MiMusica \
  --verbose
# Mensaje esperado:
# ⚠️  Tamaño del dispositivo: 128.50 GB (requiere confirmación por seguridad)
```

### Caso 2: Depuración con logs detallados

```bash
RUST_LOG=trace cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica \
  -vvv
```

### Caso 2.1: Seleccionar idioma de mensajes runtime

```bash
# Espanol (default)
cargo run -p lap-bin-provision -- --lang es list

# Ingles
cargo run -p lap-bin-provision -- --lang en list

# Alternativa por entorno
LAP_LANG=en cargo run -p lap-bin-provision -- list
```

#### Guía rápida de idioma (i18n runtime)

Reglas operativas:
1. Idioma por bandera CLI: `--lang es|en`.
2. Idioma por entorno: `LAP_LANG=es|en`.
3. Precedencia: si defines ambos, gana `--lang`.

Comandos recomendados:

```bash
# Forzar español en un comando puntual
cargo run -p lap-bin-provision -- --lang es list

# Forzar inglés en un comando puntual
cargo run -p lap-bin-provision -- --lang en list

# Dejar inglés como default de la sesión de shell
export LAP_LANG=en
cargo run -p lap-bin-provision -- list

# Sobrescribir temporalmente el default del entorno
cargo run -p lap-bin-provision -- --lang es list
```

### Caso 3: Listar dispositivos detectados

```bash
cargo run -p lap-bin-provision -- list
```

### Caso 4: Reanudar una provisión interrumpida

Si el proceso fue interrumpido (corte de luz, desconexión USB), el checkpoint atómico preservó el estado exacto. Para reanudar:

```bash
cargo run -p lap-bin-provision -- \
  resume \
  --usb /media/usuario/DISCO_USB \
  --resume ~/.lap/backups/usb_backup_in_place__cabina_a_sandisk_ultra_fit_4c530001230101117391_abcd_1234_9f31a0d2
```

El recovery compara los SHA256 reales de la USB contra el checkpoint y solo recopia los archivos faltantes o corruptos. Los archivos ya copiados correctamente **no se tocan**.

### Caso 5: Source host "sucia" con validación estricta de paridad

Si quieres bloquear el proceso cuando el source no coincide exactamente con el baseline procesado, usa `--strict-parity` (requiere `--sync`).

```bash
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica \
  --sync \
  --strict-parity
```

`--strict-parity` valida antes de mutar contenido:
1. source -> manifest: todos los hashes del source deben existir en el manifest baseline.
2. manifest -> USB: cada entrada del manifest debe existir en USB y coincidir en hash.

Si falla alguna paridad, aborta con `INVALID_CONFIG` y no modifica la USB.

Precondiciones operativas de `--strict-parity`:
1. Debe ejecutarse junto con `--sync`.
2. Debe existir un baseline de manifest generado por una provisión previa.
3. Si no existe baseline, la ejecución aborta con `INVALID_CONFIG` y primero debes correr una provisión inicial sin `--strict-parity`.

### Caso 6: Transformar musica en carpeta host para moverla a USB

Si tu necesidad es limpiar/normalizar la coleccion en host para dejarla apta para una USB legacy, este flujo ya esta cubierto con `provision` usando `--source` apuntando al host.

```bash
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica \
  --sync \
  --strict-parity \
  --verbose
```

Que hace el pipeline en este escenario:
1. Descubre audio en host (`--source`) y aplica sanitizacion de nombres ASCII legacy.
2. Normaliza/limpia audio (FFmpeg) cuando corresponde.
3. En modo `--sync`, evita reprocesar lo ya valido por hash.
4. Con `--strict-parity`, aborta si host/manifest/USB no estan en paridad antes de mutar.

Alcance actual: el comando transforma para destino USB dentro del mismo pipeline de provision.
No existe hoy un subcomando separado para "pre-transformar solo host" y luego copiar manualmente fuera del flujo de `provision`.

### Caso 7: USB ya procesada + host mixto (nuevos + duplicados)

Cuando la USB ya tiene contenido provisionado y en host hay mezcla de canciones nuevas y duplicadas, usa incremental con validacion estricta:

```bash
cargo run -p lap-bin-provision -- \
  provision \
  --usb /media/usuario/DISCO_USB \
  --source ~/MiMusica \
  --sync \
  --strict-parity \
  --verbose
```

Resultado esperado en este escenario:
1. Los duplicados por contenido (mismo hash ya registrado) se omiten en `--sync`.
2. Solo se transforman y copian entradas realmente nuevas que cumplan la politica de paridad.
3. Si hay drift entre manifest y USB, o source fuera de baseline en modo estricto, aborta antes de mutar la USB.

Flujo recomendado cuando es primera corrida sobre esa USB:
1. Ejecuta una corrida inicial sin `--strict-parity` para crear baseline.
2. Desde la segunda corrida en adelante, habilita `--sync --strict-parity` para control estricto.

## Estructura Resultante en USB

Después de ejecutar, tu USB se verá así:

```
DISCO_USB/
├── VOL_01/
│   ├── 0001_Cancion_1.mp3
│   ├── 0002_Cancion_2.mp3
│   ├── 0003_Cancion_3.mp3
│   └── ... (hasta 50 archivos)
├── VOL_02/
│   ├── 0051_Cancion_51.mp3
│   ├── 0052_Cancion_52.mp3
│   └── ... (hasta 50 archivos)
└── VOL_03/
    └── ... (archivos restantes)
```

**Importante**: Los números (0001_, 0002_, etc.) ayudan al firmware del estéreo a:
1. Mantener el orden de reproducción
2. Evitar problemas de punteros en memoria
3. Facilitar la navegación secuencial en la FAT
4. Mantener orden lexicográfico correcto después de 0999 (`0999` -> `1000`)

## Backup Automático

Antes de copiar cualquier archivo a la USB, el programa crea una copia local:

```
~/.lap/
├── backups/
│   └── usb_backup_in_place__cabina_a_sandisk_ultra_fit_4c530001230101117391_abcd_1234_9f31a0d2/
│       ├── VOL_01/
│       │   └── 0001_Cancion_1.mp3
│       └── VOL_02/
│           └── 0051_Cancion_51.mp3
├── checkpoints/
├── manifests/
└── journals/
```

El estado operativo es host-only y se crea en `~/.lap` por defecto (o `LAP_STATE_DIR`).
La USB no recibe manifiestos, journals ni checkpoints. El nombre de estado por dispositivo usa slug + hash corto para blindar colisiones.
En `--in-place-rebuild`, el backup preserva la estructura de carpetas relativa al mount para evitar colisiones por nombre plano.

## Reformat Seguro a FAT32 Legacy

Si la USB no está en FAT32 o usa un allocation unit distinto de 32 KB, puedes usar el subcomando explícito de reformateo. El flujo hace backup completo antes de borrar la USB y exige confirmación exacta del dispositivo detectado.

```bash
cargo run -p lap-bin-provision -- \
  format \
  --usb /media/usuario/DISCO_USB \
  --confirm-device /dev/sdb1 \
  --label CABINA_A
```

Si la USB ya cumple el perfil legacy, el comando no reformatea nada y sale en modo no-op. Para forzar reformateo aunque ya cumpla:

```bash
cargo run -p lap-bin-provision -- \
  format \
  --usb /media/usuario/DISCO_USB \
  --confirm-device /dev/sdb1 \
  --force-reformat
```

## Verificación Post-Provisioning

Después de provisionar, el estéreo debería:
1. ✓ Montar la USB sin errores
2. ✓ Reconocer 3 carpetas (VOL_01, VOL_02, VOL_03)
3. ✓ Reproducir archivos en orden numérico
4. ✓ No tener problemas de memoria/buffer

## Solución de Problemas

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
# Recomendado: usa el comando seguro del provisioner para crear backup primero
cargo run -p lap-bin-provision -- \
  format \
  --usb /media/usuario/DISCO_USB \
  --confirm-device /dev/sdb1
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

**Solución**: Libera espacio en tu directorio home:
```bash
# Ver tamaño de backups anteriores
du -sh ~/.lap/backups/usb_backup_*

# Eliminar backups antiguos si es necesario
rm -rf ~/.lap/backups/usb_backup_<identidad_del_dispositivo>
```

Si el preflight no puede leer metadata de un archivo que debe respaldarse, la operación falla antes de mutar contenido.

## Rendimiento

### Tiempos esperados

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
| Longitud de nombre final en USB | 32 caracteres |
| Longitud de stem durante sanitización | 64 caracteres |
| Tamaño de clúster FAT32 | 32 KB |
| Particiones soportadas | Solo MBR |
| Encoding de nombres | ASCII/ISO-8859-1 |
| Sanitización | Transliteration ASCII + Regex de ruido inicial/final + normalización a `_` |
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
$ cargo run -p lap-bin-provision -- \
  provision \
    --usb /media/user/DISK \
    --source ~/Music \
    --dry-run \
    -v

# 5. Ejecutar en real
$ cargo run -p lap-bin-provision -- \
  provision \
    --usb /media/user/DISK \
    --source ~/Music

# 6. Verificar
$ ls -la /media/user/DISK/
total 20
drwxr-xr-x  3 user user  4096 Mar  6 14:30 .
drwxr-xr-x  7 user user  4096 Mar  6 14:29 ..
drwxr-xr-x  2 user user  4096 Mar  6 14:30 VOL_01
drwxr-xr-x  2 user user  4096 Mar  6 14:30 VOL_02

# 7. (Opcional) Expulsar USB de forma segura
$ LAP_SAFE_EJECT=1 cargo run -p lap-bin-provision -- \
  provision \
    --usb /media/user/DISK \
    --source ~/Music

# 8. O manualmente:
$ eject /dev/sdb1
```

**¡Listo!** Tu USB está lista para el estéreo antiguo.
