# Índice ADR (Inmutable)

Esta carpeta almacena Registros de Decisión de Arquitectura (ADR) utilizados como registros históricos inmutables.

> Nota de contexto histórico: algunos ADR tempranos referencian rutas `src/*` del diseño monolítico previo. Esas rutas deben leerse como evidencia histórica. El runtime/documentación operativa vigente usa el workspace por crates (`crates/*`), canonizado por ADR-0012.

## Reglas

- Los ADR son append-only. No reescribas cuerpos de ADR aceptados.
- Si una decisión cambia, crea un ADR nuevo y marca enlaces con `Sustituye a` / `Sustituido por`.
- Mantén los ADR breves (contexto, decisión, consecuencias).
- Incluye actualizaciones de ADR en el mismo commit que el cambio relacionado de código/documentación.

## Ciclo de Vida

- `Propuesto`: en discusión.
- `Aceptado`: decisión activa.
- `Sustituido`: registro histórico reemplazado por un ADR más reciente.

## Nomenclatura

Usa IDs con ceros a la izquierda y slugs cortos:

- `0001-rust-project-structure.md`
- `0002-direct-file-copy.md`
- `0003-ffmpeg-normalization.md`
- `0004-quarantine-isolation.md`
- `0005-sync-sha256.md`
- `0006-docs-as-code-governance.md`
- `0007-canonical-path-validation.md`
- `0008-host-local-staging-for-in-situ-refactoring.md`
- `0009-cli-standardization.md`
- `0010-offensive-security-hardening.md`
- `0011-architecture-single-vs-workspace.md`
- `0012-thin-entrypoint-orchestrator-reporter.md`

## Plantilla

```md
# ADR 000X: Título Corto de Decisión

- Estado: Propuesto | Aceptado | Sustituido por ADR-XXXX
- Fecha: YYYY-MM-DD
- Autor: <name>
- Sustituye a: ADR-XXXX (opcional)

## 1. Contexto

Problema e incertidumbre que se están resolviendo.

## 2. Decisión

La regla que el sistema va a aplicar.

## 3. Consecuencias

- Positivas:
- Negativas:
```
