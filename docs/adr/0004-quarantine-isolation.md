# ADR 0004: Backup-First Quarantine for Untracked USB Files

- Estado: Aceptado
- Fecha: 2026-03-16
- Autor: Luis / Legacy Audio Project

## 1. Contexto

Los destinos USB pueden contener archivos del cliente no rastreados y no representados en el checkpoint. Eliminar archivos desconocidos implica riesgo contractual.

## 2. Decisión

Antes de cualquier mutación, respaldar los archivos no rastreados en el host y luego moverlos a `.legacy_quarantine/<session>/` en la USB.

## 3. Consecuencias

- Positivas:
  - Se minimiza el riesgo de pérdida de datos.
  - Se limpia la raíz de la USB para reproducción legacy determinística.
  - Se preserva la trazabilidad de auditoría.
- Negativas:
  - Sobrecarga temporal de almacenamiento en host/USB.
  - Tiempo de ejecución ligeramente mayor.
