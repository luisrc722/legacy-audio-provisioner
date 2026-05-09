# Politica de Evidencia Historica

Este directorio conserva artefactos de corridas reales (logs, salidas de herramientas, trazas de sistema) como evidencia factual de auditoria.

Reglas:

1. Los archivos en `docs/testing/evidence/` son snapshots historicos no normativos.
2. Pueden contener rutas, nombres o formatos heredados de versiones anteriores.
3. No deben editarse para "normalizar" formato, salvo para corregir corrupcion de archivo.
4. La documentacion normativa vigente esta en:
   - `README.md`
   - `docs/spec/requirements_traceability.md`
   - `docs/architecture/*.md`
5. Si un snapshot historico difiere del estado actual de implementacion, prevalece la documentacion normativa y el codigo en `crates/*`.

Objetivo: preservar integridad probatoria sin introducir ambiguedad sobre el comportamiento vigente.
