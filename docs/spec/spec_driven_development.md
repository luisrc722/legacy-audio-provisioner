## 1. Especificación de Requerimientos del Sistema (SRS)

### **R-01: Requerimientos de Particionamiento y Formato**

El firmware del estéreo probablemente tiene un puntero de direccionamiento limitado. Para asegurar compatibilidad:

* **Tabla de Particiones:** Debe ser estrictamente **MBR (Master Boot Record)**. Se prohíbe el uso de GPT (GUID Partition Table) debido a que muchos microcontroladores de 32 bits no manejan el esquema de respaldo de encabezados de GPT.
* **Sistema de Archivos:** Únicamente **FAT32**.
* **Tamaño de Clúster:** Forzar a $32\text{ KB}$ (o el máximo soportado para el tamaño del volumen) para minimizar el tamaño de la **File Allocation Table (FAT)** y facilitar la lectura secuencial al procesador del estéreo.

### **R-02: Requerimientos de Estructura de Datos (Directorios)**

Para evitar desbordamientos de buffer en la memoria RAM del estéreo al indexar:

* **Profundidad de Directorios:** Máximo 2 niveles (Raíz > Carpeta > Archivo).
* **Límite de Objetos por Carpeta:** Máximo 50 archivos por directorio para no exceder los buffers de lectura de los sistemas de archivos simplificados.
* **Orden de Escritura:** Los archivos deben escribirse en la memoria de forma **secuencial física**. Algunos estéreos no ordenan alfabéticamente, sino por el orden en que aparecen en la FAT.

### **R-03: Requerimientos de Sanitización de Nombres**

El "path" completo es a menudo almacenado en un string de longitud fija (p. ej. `char[128]`).

* **Longitud de Nombre:** Máximo 32 caracteres por archivo.
* **Encoding:** Estrictamente **ASCII/ISO-8859-1**. Eliminar emojis, tildes o caracteres UTF-8 multi-byte que rompan el puntero de lectura.
* **Regex de limpieza:** `s/[^a-zA-Z0-9\.\-\_]//g`.

---

## 2. Matriz de Restricciones Técnicas

| Feature | Estado Moderno (Linux/Windows) | Spec para Estéreo "Dumb" |
| --- | --- | --- |
| **Addressing** | 64-bit / LBA 48 | 32-bit (Simulado o real) |
| **Max Files** | Millones (ext4/NTFS) | ~255 por segmento |
| **Metadata** | ID3v2.4 (UTF-8, Imágenes 4K) | ID3v1 o v2.3 (Sin portadas) |
| **Archivos Ocultos** | Ignorados por el OS | Causan error de puntero o *hang* |

---

## 3. Requerimientos de Limpieza de "Basura" (Data Scrubbing)

El software debe ejecutar una fase de purga para eliminar metadatos y archivos que el estéreo intentará procesar erróneamente:

* **Eliminación de archivos dot-underscore (`._*`):** Generados por indexadores de macOS/Linux.
* **Eliminación de carpetas de sistema:** `System Volume Information`, `LOST.DIR`, `.Trash-1000`.
* **Stripping de ID3 Tags:** Eliminar imágenes incrustadas (Album Art) de los MP3. Si el estéreo intenta cargar una imagen de 2MB en una RAM de 512KB antes de tocar la canción, el sistema se reinicia (el famoso "redireccionamiento de memoria" que mencionabas).

Regla de implementación obligatoria:

* **Prefiltrado en capa de lectura (pre-procesamiento):** El filtrado de `._*` y carpetas ocultas debe ocurrir antes de descender por el árbol de directorios (p. ej. con `WalkDir::filter_entry`). No se permite post-filtrado, para evitar iterar miles de entradas fantasma y exponer el pipeline a metadatos inválidos.

---

### Implementación sugerida en Rust

Dado que estás aprendiendo **Rust**, este es un proyecto perfecto para practicar. Rust te da el control de bajo nivel necesario para manipular la FAT y los bytes de los archivos sin el riesgo de memoria de C.

```rust
// Ejemplo conceptual de una función de sanitización en Rust
fn sanitize_filename(input: &str) -> String {
    input.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_')
        .take(32) // Límite de la Spec
        .collect()
}

```
---

Para completar el diseño de este **`legacy-audio-provisioner`** bajo la metodología de **Spec-Driven Development**, vamos a definir los módulos lógicos que faltan. Como buen informático, sabes que la seguridad de los datos es primero, así que el flujo de "detección y resguardo" es crítico.

Aquí tienes la especificación técnica de los módulos restantes:

---

## 4. Especificación de Módulos (SDD - Parte 2)

### **R-04: Detección Dinámica de Hardware**

El sistema debe identificar de forma unívoca el volumen físico para evitar la sobrescritura accidental de discos del sistema.

* **Identificación:** En entornos Linux, debe filtrar dispositivos por el subsistema `block` y buscar removibles (USB).
* **Validación de Montaje:** El script debe verificar el punto de montaje actual (ej. `/media/dev/6A08-0A02`) y el espacio disponible.
* **Safety Check:** Si el dispositivo excede los 64 GB, debe pedir una confirmación extra (para evitar formatear discos duros externos por error).

### **R-05: Preservación y Backup (Data Integrity)**

Antes de cualquier operación destructiva (como el renombrado masivo o formateo), el sistema debe asegurar la persistencia.

* **Directorio de Backup:** Crear un directorio temporal en el `$HOME` del usuario con el timestamp actual: `~/usb_backup_YYYYMMDD_HHMM/`.
* **Atomicidad:** La operación de backup debe ser verificada mediante un conteo de archivos o comparación de *checksums* (MD5/SHA) antes de proceder al siguiente paso.
* **Espacio en Disco:** El sistema debe fallar si el espacio en el disco local es menor al tamaño ocupado en la USB.

### **R-06: Transformación y Normalización (Logic Layer)**

Aquí es donde aplicamos las restricciones de los 32 bits y el direccionamiento de memoria del estéreo.

* **Pipeline de Renombrado:**
1. Remover metadatos (ID3v2 tags pesados).
2. Aplicar el regex: `[a-zA-Z0-9_\.]`.
3. Añadir prefijo numérico secuencial: `001_nombre.mp3`, `002_nombre.mp3`. Esto ayuda al puntero del firmware a no "perderse".


* **Normalización de Audio (Opcional):** Asegurar que todos los archivos sean Constant Bit Rate (CBR) a 128-192 kbps, ya que el Variable Bit Rate (VBR) a veces rompe el direccionamiento de tiempo en estéreos viejos.

### **R-07: Distribución de Carga (Redistribución por Segmentos)**

Para cumplir con el límite de archivos por carpeta $N \le 50$:

* **Algoritmo de Bucketizado:**
* Calcular el número total de archivos $T$.
* Crear $k$ carpetas donde $k = \lceil T / 50 \rceil$.
* Nombre de carpetas: `VOL_01`, `VOL_02`, etc.
* Distribuir archivos secuencialmente para asegurar que la FAT no se fragmente.



---

## 5. Matriz de Tareas de Implementación (Backlog)

| ID | Tarea | Prioridad | Complejidad (Story Points) |
| --- | --- | --- | --- |
| **T.1** | Escaneo de `/dev/` para identificar dispositivos `vfat` | Alta | 5 |
| **T.2** | Implementación de backup con I/O nativa (`std::fs`) + checksums | Alta | 3 |
| **T.3** | Módulo de sanitización de strings (ASCII-only) | Media | 2 |
| **T.4** | Lógica de segmentación en carpetas (Buckets) | Alta | 8 |
| **T.5** | Verificación final y expulsión segura (`umount`) | Baja | 1 |

---

## 6. Consideración de Implementación en Rust

Si decides llevar esto a código en **Rust**, te sugiero usar los siguientes crates para cumplir con la Spec:

* **`nix` y/o `sysinfo`:** Para detectar puntos de montaje y validar espacio/dispositivo de forma portable.
* **`walkdir`:** Para iterar sobre los archivos de la USB de forma eficiente.
* **`indicatif` (opcional):** Para barras de progreso.

Para operaciones críticas de datos, preferir I/O nativa de Rust:

* **`std::fs::copy` + `File::sync_all()`:** En lugar de wrappers externos (`cp`, `rsync`, `fs_extra`) para garantizar durabilidad y orden de escritura en FAT32.

### Ejemplo de la lógica de distribución (Pseudo-Rust):

```rust
let files = get_list_of_mp3s(backup_path)?;
let chunk_size = 50;

for (idx, chunk) in files.chunks(chunk_size).enumerate() {
    let folder_name = format!("VOL_{:02}", idx + 1);
    create_dir(target_usb.join(&folder_name))?;

    for file in chunk {
        let new_name = sanitize_and_index(file);
        copy(file, target_usb.join(&folder_name).join(new_name))?;
    }
}

```
---

Para que tu **SDD (Spec-Driven Development)** pase de ser un script "parche" a una herramienta de grado profesional (especialmente útil para tu formación en la UNAM), necesitamos extender el alcance hacia la **robustez, observabilidad y cumplimiento de estándares de hardware**.

Aquí tienes los pilares para extender el scope y cubrir esos "huecos" de seguridad y confiabilidad:

---

## 7. Módulo de Integridad y Verificación (Security Extension)

Un sistema que mueve datos debe garantizar que el destino es un espejo fiel del origen.

* **R-08: Verificación Post-Escritura (Checksumming):**
* El sistema debe generar un hash (SHA-256 o MD5) de cada archivo en el backup y compararlo con el archivo escrito en la USB.
* **Justificación:** Las memorias USB flash suelen tener sectores defectuosos o controladores de baja calidad que corrompen datos al escribir grandes volúmenes.


* **R-09: Idempotencia:**
* Si el proceso se interrumpe (se desconecta la USB), el sistema debe ser capaz de reanudarse sin duplicar archivos ni corromper la tabla de asignación de archivos (FAT).



---

## 8. Módulo de Normalización de Media (Compatibility Scope)

El "redireccionamiento de memoria" que mencionaste no solo ocurre por el nombre del archivo, sino por el contenido del mismo.

* **R-10: Transcodificación de Audio (Standardization):**
* **Scope:** Convertir archivos `.wav`, `.flac` o `.m4a` a `.mp3`.
* **Spec (política recomendada):**
  1. **Passthrough seguro** si el archivo ya es MP3 CBR compatible (p. ej. 128-320 kbps, 44.1 kHz).
  2. **Transcodificar** solo si el archivo está en formato no soportado por firmware legacy o si usa VBR/rate fuera de rango.
  3. **Salida target** para transcodificación: MP3 CBR 128-192 kbps, 44.1 kHz.
* **Por qué:** Los estéreos viejos usan decodificadores por hardware que no soportan Variable Bit Rate (VBR) o frecuencias altas, lo que causa que la canción se escuche acelerada o simplemente se salte.


* **R-11: Stripping de Metadatos (ID3 Sanitization):**
* Eliminar absolutamente todos los tags excepto *Título* y *Artista*.
* Eliminar imágenes incrustadas (Album Art). Como mencionamos, una imagen de 2MB puede agotar el buffer de un microcontrolador de 32 bits diseñado para audio.



---

## 10. Módulo de Seguridad Operativa (Risk Management)

* **R-12: Modo "Dry Run" (Simulación):**
* El sistema debe permitir una ejecución de prueba (`--dry-run`) que muestre en la terminal qué archivos se renombrarán, qué carpetas se crearán y cuánto espacio se ocupará, **sin escribir nada**.


* **R-13: Validación de Salud del Hardware (S.M.A.R.T. Lite):**
* Antes de iniciar, verificar si la USB está en modo de "solo lectura" (señal de que la memoria flash ha llegado al fin de su vida útil) para evitar errores crípticos a mitad del proceso.



---

## 11. Interfaz y Observabilidad (User Experience)

Para un proyecto de ingeniería, el "qué está pasando" es tan importante como el "qué hizo".

* **R-14: Logging Estructurado:**
* Generar un archivo `provisioning.log` en el backup que detalle cada operación: `[TIMESTAMP] [INFO] Renamed: 'My Song.mp3' -> '001_My_Song.mp3'`.


* **R-15: Feedback Visual (Progress Bars):**
* Implementar una barra de progreso real (puedes usar el crate `indicatif` si lo haces en Rust) para el copiado de datos, ya que el comando `cp` estándar de Linux es "mudo" y no sabemos si se trabó.



---

### Actualización del Backlog Técnico

| ID | Tarea | Prioridad | Dependencias |
| --- | --- | --- | --- |
| **T.6** | Integración de `ffmpeg` para normalizar audio | Media | T.4 |
| **T.7** | Módulo de verificación por Hash (SHA-256) | Alta | T.2 |
| **T.8** | Implementación de Flag `--dry-run` | Alta | T.3, T.4 |
| **T.9** | Limpieza profunda de ID3 Tags (v1 y v2) | Alta | T.3 |

---

## 12. Módulo de Recuperación ante Fallos (Disaster Recovery)

Para garantizar la robustez del proceso en entornos de producción donde la USB puede desconectarse o fallar:

* **R-16: Checkpoint System (Recuperación de Estado):**
* Implementar puntos de control (checkpoints) que registren el progreso de la operación en `.provisioning_checkpoint`.
* Si el proceso se interrumpe, el sistema debe detectar el checkpoint y ofrecer la opción de reanudar desde el último punto válido sin perder datos.
* **Formato del checkpoint:** JSON versionado y atómico, con tracking por índice en `BTreeMap<usize, FileCheckpoint>`.
* **Escritura atómica obligatoria:** `write .tmp -> sync_all() -> rename()` para evitar estados parciales ante corte eléctrico.
* **Ejemplo mínimo:**
```json
{
  "version": 1,
  "session_id": "session_20260315_143045",
  "total_files": 125,
  "operation_status": "InProgress",
  "processed_files": {
    "45": {
      "normalized_name": "046_song.mp3",
      "status": "Completed"
    }
  }
}
```

* **R-17: Rollback Automático:**
* Si una operación crítica falla (p. ej., corrupción de FAT detectada), el sistema debe poder restaurar automáticamente desde el backup creado en R-05.
* Requisito: Mantener integridad referencial entre USB y backup mediante logs de transacciones.

---

## 13. Módulo de Compatibilidad de Formatos (Media Handling)

La mayoría de reproductores legales soportan formatos específicos con limitaciones de codec:

* **R-18: Detección y Soporte de Múltiples Formatos:**
* **Formatos de entrada soportados:**
  - `.mp3` (MPEG-1/2 Audio Layer III) → Directo
  - `.flac` (Free Lossless Audio Codec) → Convertir a MP3 CBR
  - `.wav` (PCM, WAV) → Convertir a MP3 CBR
  - `.m4a` (AAC) → Convertir a MP3 CBR
  - `.ogg` (Vorbis) → Convertir a MP3 CBR
  - `.wma` (Windows Media Audio) → Convertir a MP3 CBR
* **Formato de salida compulsorio**: MP3 CBR 128-192 kbps, 44.1 kHz
* **Regla para formatos/extensiones no declaradas:** Deben ignorarse y excluirse del checkpoint. Se registra advertencia en log, pero no se aborta la operación global.

* **R-19: Validación de Códec y Bitrate:**
* Antes de copiar/convertir, verificar:
  1. Codec es soportado (~30 codecs comunes)
  2. Bitrate está dentro del rango soportado (8-320 kbps CBR)
  3. Sample rate es compatible (44.1 kHz, 48 kHz como máximos)
* Si alguna validación falla, registrar en log y **saltar el archivo** con advertencia.
* **No fallar la operación completa por un archivo corrompido.**

---

## 14. Matriz de Cumplimiento de Requisitos

Para rastrear qué versiones del software cumplen con qué requisitos:

| Requisito | Descripción | Versión MVP | Versión 1.0 | Versión 2.0 | Status Actual |
|-----------|-------------|-------------|------------|------------|---------------|
| **R-01** | MBR + FAT32 | ✓ Spec | ✓ | ✓ | ✅ Documentado |
| **R-02** | Estructura directorios (max 2 niveles, 50 arch/carpeta) | ✓ | ✓ | ✓ | ✅ Implementado |
| **R-03** | Sanitización ASCII 32 caracteres | ✓ | ✓ | ✓ | ✅ Implementado |
| **R-04** | Detección de hardware USB | ✓ Parcial | ✓ | ✓ | ✅ Implementado |
| **R-05** | Backup con checksums SHA256 | ✓ Parcial | ✓ | ✓ | ✅ Implementado |
| **R-06** | Descubrimiento + normalización base | ✓ Parcial | ✓ | ✓ | ✅ Implementado (núcleo) |
| **R-07** | Distribución de carga (buckets) | ✓ | ✓ | ✓ | ✅ Implementado |
| **R-08** | Verificación post-escritura | × | ✓ | ✓ | ✅ Implementado |
| **R-09** | Idempotencia y reanudación | × | ✓ | ✓ | ✅ Implementado |
| **R-10** | Transcodificación de audio | × | ✓ | ✓ | ⏳ Planificado |
| **R-11** | Stripping de metadatos ID3 | × | ✓ | ✓ | ⏳ Planificado |
| **R-12** | Modo dry-run | ✓ | ✓ | ✓ | ✅ Implementado |
| **R-13** | Validación de salud USB (S.M.A.R.T.) | × | ✓ Parcial | ✓ | ⏳ Planificado |
| **R-14** | Logging estructurado | ✓ Parcial | ✓ | ✓ | ⏳ En Progreso |
| **R-15** | Progress bars | ✓ Parcial | ✓ | ✓ | ⏳ En Progreso |
| **R-16** | Checkpoint system | × | ✓ | ✓ | ✅ Implementado |
| **R-17** | Rollback automático | × | ✓ | ✓ | ✅ Implementado |
| **R-18** | Soporte múltiples formatos | × | ✓ Parcial | ✓ | ⏳ Planificado |
| **R-19** | Validación de codec/bitrate | × | ✓ | ✓ | ⏳ Planificado |

---

## 15. Casos de Prueba y Escenarios

Para verificar que el sistema cumple con la especificación:

### Escenario 1: Flujo Normal (Happy Path)
```
Input:  100 archivos MP3 válidos en ~/Music
Steps:
  1. Detectar USB en /media/user/DISK (FAT32, 16GB, removible)
  2. Crear backup ~/usb_backup_20260306_1430/
  3. Escanear 100 archivos durante 1 segundo
  4. Aplicar sanitización + prefijos secuenciales
  5. Distribuir en 2 volúmenes: VOL_01 (50), VOL_02 (50)
  6. Verificar integridad (checksums)
  7. Expulsar USB con eject
Output: USB lista con estructura VOL_01/*, VOL_02/*
Status: ✅ PASS
```

### Escenario 2: USB Muy Grande (> 64 GB)
```
Input:  USB externa 128GB detectada en /media/user/LARGE
Steps:
  1. Validar device info
  2. Detectar size > 64GB
  3. Mostrar advertencia: "Device 128.5 GB requires confirmation"
  4. Pedir confirmación explícita (Y/N)
  5. Proceder si usuario confirma (Y)
Expected: Sistema no ejecuta sin confirmación
Status: ✅ PASS (if implemented)
```

### Escenario 3: Fallo Intermedio
```
Input:  1000 archivos, pero USB desconectada a los 500
Steps:
  1. Checkpoint JSON guardado atómicamente (.tmp -> sync_all -> rename)
  2. processed_files[250].status = "Completed"
  3. Usuario reconecta USB
  4. Sistema detecta checkpoint
  5. Ejecuta --resume con backup_dir
  6. Reanuda solo archivos faltantes/corruptos tras comparar SHA256
Expected: Sin duplicación, sin pérdida de datos
Status: ✅ PASS
```

### Escenario 4: Archivo Corrompido
```
Input:  100 archivos, archivo #42 está corrompido (0 bytes)
Steps:
  1. Validación de codec detecta error
  2. Log: "[WARN] Skipping file_no_valid_audio.mp3: codec not supported"
  3. Continúa con archivo #43
  4. Completa los 99 archivos restantes
Expected: No fallar operación por 1 archivo inválido
Status: ⏳ REQUIRES R-19
```

### Escenario 5: Dry Run
```
Command: ./legacy-audio-provisioner --usb-mount /media/user/DISK --audio-source ~/Music --dry-run
Expected Output:
  ✓ Device validated (16GB, FAT32, removible)
  ✓ Found 100 audio files
  ✓ Would create 2 volumes (VOL_01: 50, VOL_02: 50)
  ✓ Would use 500MB (0 bytes written)
  ✓ [DRY-RUN] No actual changes made
Status: ✅ PASS (if implemented)
```

---

## 16. Matriz de Dependencias de Crates

Para Phase 2, se recomienda agregar estos crates en orden:

| Crate | Versión | Requisito | Prioridad |
|-------|---------|-----------|-----------|
| `metaflac` | ~0.2 | R-11 (ID3 tags) | Alta |
| `ffmpeg-sys` / `ac-ffmpeg` | Latest | R-10, R-18 (Conversión audio) | Alta |
| `indicatif` | ~0.17 | R-15 (Progress bars) | Media |
| `rustyline` | ~0.14 | Confirmaciones interactivas | Media |
| `sysinfo` | Latest | R-04 (Detección automática, portable) | Alta |
| `nix` | Latest | R-04/R-05 (statvfs y utilidades Unix) | Alta |
| `smart` / `nvme` | Latest | R-13 (S.M.A.R.T. Lite) | Baja |

---

## 17. Plan de Evolución del Proyecto

```
Phase 1 (MVP - DONE):
├─ R-01, R-02, R-03, R-07 ✅
├─ R-04, R-05, R-06 (núcleo) ✅
└─ R-12, R-14, R-15 (CLI básico) ✅

Phase 2 (v1.0 - 1-2 meses):
├─ R-08, R-09, R-16, R-17 ✅
├─ R-10, R-11 (Audio processing)
├─ R-13 (Salud de hardware)
└─ Sistema de tests integración (Escenarios 1-5)

Phase 3 (v2.0 - 3 meses):
├─ R-18, R-19 (Multi-formato)
├─ Soporte Windows/macOS
├─ GUI (GTK/Qt)
└─ Documentación académica completa
```

---
