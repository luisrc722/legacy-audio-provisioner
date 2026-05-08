# ADR 0003: FFmpeg Normalization for Legacy Compatibility

- Estado: Aceptado
- Fecha: 2026-03-16
- Autor: Luis / Legacy Audio Project

## 1. Contexto

Los estéreos legacy fallan con códecs inconsistentes, metadatos no soportados, casos borde de VBR y complejidad de contenedor.

## 2. Decisión

Normalizar la salida mediante FFmpeg/ffprobe para producir MP3 determinístico, compatible con dispositivos legacy y sin metadatos.

## 3. Consecuencias

- Positivas:
  - Reproducción estable en firmware legacy con recursos limitados.
  - Menor variabilidad entre catálogos de origen.
- Negativas:
  - Dependencia de la herramienta FFmpeg.
  - CPU e I/O adicionales durante la normalización.
