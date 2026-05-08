#!/usr/bin/env bash
set -euo pipefail

echo "=== Telemetría de Desgaste NAND y Optimización I/O [R-02-010] ==="

if ! command -v strace >/dev/null 2>&1; then
    echo "ERROR: 'strace' no está instalado. Ejecuta: sudo apt install strace"
    exit 1
fi

# Configuración del entorno de prueba
WORK_DIR=$(mktemp -d -t lap_telemetry_XXXXXX)
USB_TARGET="${1:-}"
SOURCE_DIR="${2:-}"

# Validación estricta de argumentos
if [ -z "$USB_TARGET" ] || [ -z "$SOURCE_DIR" ]; then
    echo "ERROR: Faltan argumentos obligatorios."
    echo "Uso estricto: $0 <usb_mount_point> <source_dir>"

    # Intento de autodescubrimiento solo para mostrar ayuda útil
    DETECTED_USB=$(lsblk -rno RM,FSTYPE,MOUNTPOINTS | awk '$1 == "1" && $2 ~ /vfat|fat32/ && $3 != "" {print $3; exit}')
    if [ -n "$DETECTED_USB" ]; then
        echo "Info: Se detectó una posible USB en $DETECTED_USB"
    fi
    exit 1
fi

if [ ! -d "$USB_TARGET" ]; then
    echo "ERROR: El destino USB '$USB_TARGET' no existe o no está montado."
    exit 1
fi

if [ ! -d "$SOURCE_DIR" ]; then
    echo "ERROR: El origen de audio '$SOURCE_DIR' no existe."
    exit 1
fi

if [ "$(realpath "$USB_TARGET")" = "$(realpath "$SOURCE_DIR")" ]; then
    echo "ERROR: source_dir no puede ser igual al usb_mount_point."
    exit 1
fi

echo "-> 0. Modo REAL (sin simulación): no se borran datos y no se generan fixtures"
TOTAL_FILES=$(find "$SOURCE_DIR" -type f \
    \( -iname "*.mp3" -o -iname "*.flac" -o -iname "*.wav" -o -iname "*.m4a" -o -iname "*.ogg" \) \
    | wc -l | tr -d ' ')

if [ "$TOTAL_FILES" -eq 0 ]; then
    echo "ERROR: no se encontraron archivos de audio en '$SOURCE_DIR'."
    exit 1
fi

echo "-> 1. Fuente real detectada: $TOTAL_FILES archivos en $SOURCE_DIR"

echo "-> 2. Compilando binario en modo release para telemetría pura..."
cargo build -p lap-bin-provision --release -q

echo "-> 3. Ejecutando Provisión Masiva con intercepción de Syscalls (strace)..."
START_TIME=$(date +%s%N)

# Ejecutamos strace capturando y resumiendo solo las llamadas 'fsync' (que Rust emite al llamar sync_all)
strace -c -e trace=fsync target/release/lap-bin-provision \
    provision --usb "$USB_TARGET" --source "$SOURCE_DIR" --sync \
    > "$WORK_DIR/stdout.log" 2> "$WORK_DIR/strace_summary.txt"

END_TIME=$(date +%s%N)
LATENCY_MS=$(((END_TIME - START_TIME) / 1000000))

echo "-> 4. Extrayendo métricas del Kernel..."

# strace -c saca una tabla. Buscamos la línea de 'fsync' y extraemos la columna de llamadas (calls)
FSYNC_CALLS=$(awk '$NF == "fsync" {print $(NF-1)}' "$WORK_DIR/strace_summary.txt" | tail -1)
if [ -z "$FSYNC_CALLS" ]; then FSYNC_CALLS=0; fi

RATIO=$(awk "BEGIN {print $FSYNC_CALLS / $TOTAL_FILES}")
LIMIT=0.1

echo ""
echo "=== RESULTADOS DE AUDITORÍA ==="
echo "Total de archivos procesados : $TOTAL_FILES"
echo "Llamadas fsync al kernel     : $FSYNC_CALLS"
echo "Ratio fsync/archivo          : $RATIO (Límite: <= $LIMIT)"
echo "Latencia total del lote      : ${LATENCY_MS} ms"
echo "Artefactos de ejecución      : $WORK_DIR"

# Validación de Criterios de Aceptación
COMPLIANT=$(awk "BEGIN {if ($RATIO <= $LIMIT) print 1; else print 0}")

if [ "$COMPLIANT" -eq 1 ]; then
    echo "✅ [VERIFIED]: La mitigación de Write Amplification es EXITOSA."
    echo "El desgaste NAND se redujo a la fracción permitida por R-02-010."
    exit 0
else
    echo "❌ [FAILED]: El ratio de I/O supera el límite. Amplificación de escritura detectada."
    exit 1
fi
