# ADR 0002: Direct File Copy Baseline

- Estado: Sustituido por ADR-0005
- Fecha: 2026-03-16
- Autor: Luis / Legacy Audio Project

## 1. Contexto

La implementación inicial priorizó la velocidad de entrega y partió de una semántica de copia directa host-a-USB.

## 2. Decisión

Usar un comportamiento de copia simple como línea base temprana mientras la estrategia de endurecimiento aún se definía.

## 3. Consecuencias

- Positivas:
  - Entrega inicial rápida.
  - Ruta de ejecución simple.
- Negativas:
  - Sin identidad criptográfica para deduplicación/sincronización incremental.
  - Sobrecarga por reprocesamiento completo en cada ejecución.
  - Garantías de integridad débiles para catálogos grandes.
