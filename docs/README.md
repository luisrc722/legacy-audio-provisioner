# Índice de Documentación

Indice canonico de documentacion para el workspace `v0.3.0`.

## Fuentes de Verdad

- Arquitectura y decisiones activas: `docs/adr/`.
- Especificacion funcional y trazabilidad: `docs/spec/` (incl. tech_spec y visual diagrams).
- Implementacion tecnica y contratos: `docs/contracts/`, `docs/architecture/` (R-XX notes).
- Evidencia de pruebas: `docs/testing/`.
- Contexto historico/no normativo: `docs/archive/` (documentos sustituidos marcados claramente).

Politica de rutas de codigo:
- Referencias operativas activas deben apuntar a `crates/*`.
- Referencias `src/*` fuera de `docs/archive/` solo se aceptan en ADR historicos inmutables para contexto de decisiones previas.

## Quick Navigation

### Operacion Y Release

- `README.md`
- `CHECKLIST.md`
- `CHANGELOG.md`

### Arquitectura

- `docs/adr/README.md`
- `docs/adr/0001-rust-project-structure.md`
- `docs/adr/0002-direct-file-copy.md`
- `docs/adr/0003-ffmpeg-normalization.md`
- `docs/adr/0004-quarantine-isolation.md`
- `docs/adr/0005-sync-sha256.md`
- `docs/adr/0006-docs-as-code-governance.md`
- `docs/adr/0007-canonical-path-validation.md`
- `docs/adr/0008-host-local-staging-for-in-situ-refactoring.md`
- `docs/adr/0009-cli-standardization.md`
- `docs/adr/0010-offensive-security-hardening.md`
- `docs/adr/0011-architecture-single-vs-workspace.md`
- `docs/adr/0012-thin-entrypoint-orchestrator-reporter.md`

### Especificacion Y Gobernanza

- `docs/spec/OPERATIONAL_DECISIONS.md` (AD-01 a AD-09, políticas normativas)
- `docs/spec/sdd_edge_cases_phase2.md`
- `docs/spec/requirements_traceability.md`
- `docs/spec/tech_spec.md` (Consolidación técnica v0.3.0)
- `docs/spec/requirements_diagrams_visual.md` (Visual de R-01 a R-17)
- `docs/contracts/design_by_contract.md`
- `CONTRIBUTING.md`

### Pruebas

- `docs/testing/integration_tests.md`
- `docs/testing/pbt_and_e2e_test_plan.md`
- `docs/testing/objective_audit_rubric.md`
- `docs/testing/audit_scorecard_2026-05-09.md`
- `docs/testing/evidence/README.md` (politica de evidencia historica)

### Guias

- `docs/guides/usage.md`

### Seguridad

- `docs/adr/0010-offensive-security-hardening.md`
- `docs/spec/requirements_traceability.md` (categorias `R-05` y `R-06`)

### Contexto Historico & Notas Arquitectonicas

**Notas Activas de Arquitectura** (implementación de requerimientos R-XX):
- `docs/architecture/architecture_overview.md` (Visión general del sistema)
- `docs/architecture/R04_strict_hardware_validation.md` (Validación hardware, prevención de daño)
- `docs/architecture/R06_audio_normalization_ffmpeg.md` (Normalización audio, compatibilidad legacy)
- `docs/architecture/R16_R17_checkpoint_and_recovery.md` (Checkpoint atómico + recovery granular)
- `docs/architecture/R16_sync_before_unmount_and_poweroff.md` (Sync POSIX para robustez FAT32)

**Archivos Consolidados/Sustituidos** (ver versiones consolidadas arriba):
- `docs/archive/atomic-json-checkpoint_superseded_by_R16_R17.md`
- `docs/archive/checkpoint_recovery_superseded_by_R16_R17.md`
- `docs/archive/use-json-checkpoint-for-recovery_superseded_by_R16_R17.md`
- `docs/archive/normalization-destructive-ffmpeg_superseded_by_R06.md`
- `docs/archive/force-normalization-through-ffmpeg_superseded_by_R06.md`
- `docs/archive/MODULAR_ARCHITECTURE_ROADMAP_reference_post_ADR0011.md` (Sustituido por ADR-0011)

**Otros Archivos de Auditoría e Historiales**:
- `backups/docs_archive_20260324_194040.tar.gz` (snapshot comprimido del `src/` monolítico retirado en Fase D)
- `docs/archive/README.md`
- `docs/archive/spec_driven_development_legacy_numbering_reference.md` (referencia legacy, no normativa)
- `docs/archive/AUDIT_MAIN_RS.md`
- `docs/archive/AUDIT_RESOLUTION_REPORT.md`
- `docs/archive/DEPENDENCIES_AUDIT.md`
- `docs/archive/GUIDE_SOFTWARE_DEVELOPMENT.md`
- `docs/archive/PROJECT_SUMMARY.md`
- `docs/archive/RELEASE_NOTES_v1_1_0.md` (obsoleto post-workspace)
- `docs/archive/SECURITY_HARDENING_2026-03-17.md` (reporte puntual)
- `docs/archive/SECURITY_INTEGRATION_2026-03-17.md` (reporte puntual)

## Estado de Markdown en Raíz

Activos en raiz (normativos):

- `README.md`
- `CONTRIBUTING.md`
- `CHECKLIST.md`
- `CHANGELOG.md`

Archivados (historicos/no normativos):

- `docs/archive/RELEASE_NOTES_v1_1_0.md`
- `docs/archive/SECURITY_HARDENING_2026-03-17.md`
- `docs/archive/SECURITY_INTEGRATION_2026-03-17.md`

## Reglas de Mantenimiento

1. Si se agrega, renombra o mueve un `.md`, actualizar este indice en el mismo cambio.
2. Cambios de arquitectura requieren ADR nuevo o estado `Sustituido` en ADR previo.
3. Cambios funcionales deben reflejarse en `docs/spec/requirements_traceability.md`.
4. Antes de release, validar consistencia con `CHECKLIST.md`.
5. Cambios de estructura de entrypoint/orquestacion/progreso deben reflejar ADR-0012 y su trazabilidad (`R-01-006`, `R-01-007`).
