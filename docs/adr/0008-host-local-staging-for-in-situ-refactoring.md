# ADR 0008: Host Local Staging for In-Situ Refactoring

- **Estado:** Aceptado
- **Fecha:** 2026-03-16
- **Requirement:** R-31

## 1. Contexto

Refactorización in-situ significa recibir una USB legacy como única fuente de música y devolver ese mismo dispositivo limpio y normalizado.

Procesar directamente sobre FAT32 en una USB legacy incrementa riesgo operativo por:

1. fragilidad del filesystem FAT32 ante cortes/interrupciones,
2. inestabilidad del bus USB frente a almacenamiento interno,
3. latencia/I/O variable en controladores USB de baja calidad.

El factor determinante no es asumir una tecnología específica (SSD), sino separar el trabajo intensivo de audio hacia almacenamiento local del host con mayor control de integridad.

## 2. Decisión

Adoptar un pipeline de staging local en host storage para R-31:

1. **Ingesta copy-only:** copiar audio desde origen (USB sucia o carpeta de entrada) a staging local sin mutar el origen.
2. **Terminología canónica:** usar "host storage" / "local staging area" en lugar de asumir "SSD".
3. **Trazabilidad:** registrar relación `source -> staging` con hash SHA256 por archivo.
4. **Mutación diferida de USB:** la cuarentena y escritura normalizada se ejecutan solo en la fase de provisión posterior.
5. **Hardware-agnóstico:** el staging puede residir en HDD, SSD, NVMe o RAM-disk.

## 3. Consecuencias

**Positivas:**
- Reduce riesgo de corrupción al evitar normalización directa sobre FAT32.
- Mejora resiliencia ante desconexión USB durante etapas de CPU/I/O intensivo.
- Claridad documental: la arquitectura deja de depender de una asunción de hardware moderno.

**Negativas:**
- Requiere espacio libre adicional temporal en host storage.
- En HDD mecánico, la ingesta puede ser más lenta que en SSD/NVMe.

## 4. Relación con Otros ADRs

| ADR | Relación |
| :--- | :--- |
| ADR-0004 Quarantine Isolation | R-31 mantiene backup-first y cuarentena antes de mutar datos legacy |
| ADR-0005 Sync SHA256 | R-31 reutiliza hash SHA256 para trazabilidad y sincronización segura |
| ADR-0007 Canonical Path Validation | R-31 hereda validaciones para evitar circularidad origen/destino |
