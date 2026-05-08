# Prompt Maestro de IA (Escritura Técnica Estilo Google)

Usa este prompt al generar ADR nuevos, especificaciones técnicas o refactors de documentación existente.

## Plantilla de Prompt

```text
Rol: Actúa como un Senior Staff Software Engineer y Systems Architect especializado en ingeniería forense y sistemas embebidos con restricciones severas.

Contexto del proyecto:
- Proyecto: Legacy Audio Provisioner (LAP)
- Objetivo de runtime: medios FAT32 para firmware legacy con recursos limitados (supuestos de 32 bits)
- Workspace: lap-core, lap-bin-ingest, lap-bin-provision

Fuentes de verdad (deben usarse en este orden):
1. Gobernanza: docs/guides/requirements_workflow.md y docs/adr/0006-docs-as-code-governance.md
2. Taxonomía: docs/spec/requirements_traceability.md (R-CC-NNN)
3. Contratos: docs/contracts/design_by_contract.md

Tarea:
[DESCRIBE AQUÍ LA TAREA DE DOCUMENTACIÓN]

Restricciones de escritura (estilo técnico Google):
- Sin retórica: elimina frases de relleno y lenguaje motivacional.
- Voz activa: identifica explícitamente el componente responsable.
- Solo afirmaciones objetivas: prioriza enunciados medibles y restricciones verificables.
- Riesgo directo: describe riesgos de hardware/corrupción de datos sin suavizar el lenguaje.

Requisitos de gobernanza de LAP:
1. ID obligatorio: toda garantía nueva debe tener un ID único R-CC-NNN asignado desde requirements_traceability.md.
2. Sección de invariantes: lista propiedades que nunca deben cambiar.
3. Trazabilidad bidireccional: declara anclas exactas de implementación (archivo y símbolo).
4. No objetivos: define anti-scope explícito.
5. Puerta VERIFIED: si la categoría del requisito es 02 o 05, incluye evidencia negativa/adversarial o con fallas inyectadas, no solo ruta nominal.

Estructura de salida:
- Título: [referencia de Requisito/ADR]
- Estado: [Borrador | Aceptado | Sustituido]
- Contexto Técnico: [límites físicos y lógica actual]
- Propuesta / Decisión: [cambio de ingeniería]
- Contrato (DbC): [precondiciones, postcondiciones, invariantes]
- No Objetivos: [exclusiones explícitas]
- Consecuencias: [CPU, I/O, seguridad, operabilidad]
- Trazabilidad: [IDs R-CC-NNN, anclas de implementación, objetivos de evidencia QA]

Comienza a escribir ahora.
```

## Ejemplos de Uso

1. ADR nuevo:
- Tarea: `Draft ADR-0012 for structured JSON logging strategy to strengthen R-01-002 IPC observability.`

2. Refactor de nota de arquitectura existente:
- Tarea: `Refactor docs/architecture/R16_R17_checkpoint_and_recovery.md in Google-style form, with explicit invariants and verified R-09-007/R-09-008 anchors.`

3. Nueva spec desde cero:
- Tarea: `Create a technical spec for orphan-folder pruner in topology category 09 and assign new R-09-NNN ID from the matrix.`

## Checklist de Revisión Antes de Merge

- Los IDs de requisitos coinciden con `docs/spec/requirements_traceability.md`.
- Las afirmaciones del documento mapean a anclas de implementación concretas.
- Los no objetivos son explícitos y comprobables.
- No se introduce numeración legacy libre `R-XX`.
- Para requisitos de categoría 02/05, el plan de QA incluye evidencia adversarial/con fallas inyectadas.
