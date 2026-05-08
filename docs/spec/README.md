# Especificaciones (Documentos Normativos)

Este directorio contiene las especificaciones de requisitos, especificaciones de diseño y guías de implementación para el sistema Legacy Audio Provisioner.

## Documentos Maestros

| Archivo | Propósito | Audiencia |
|------|---------|----------|
| `OPERATIONAL_DECISIONS.md` | Políticas operativas (AD-01 a AD-08) para Sync, integridad, transaccionalidad, detección de fraude de hardware, normalización, sanitización, cuarentena e IPC | Arquitectos, operaciones, desarrolladores |
| `requirements_traceability.md` | Matriz maestra de requisitos con mapeo ISO/NIST/GDPR | Líderes de proyecto, auditores |
| `sdd_edge_cases_phase2.md` | Extensión de especificación para casos borde de Fase 2 (R-18 a R-22) | Desarrolladores, QA |
| `tech_spec.md` | Consolidación de arquitectura técnica para v0.3.0 | Desarrolladores, arquitectos |
| `requirements_diagrams_visual.md` | Diagramas visuales de requisitos R-01 a R-17 | Todos los stakeholders |

## Navegación y Referencias Cruzadas

- **Detalle de Requisitos**: Consulta `requirements_traceability.md` para el mapeo completo de R-01 a R-36
- **Decisiones de Implementación**: Para decisiones de arquitectura, consulta `docs/adr/`
- **Notas de Implementación**: Para detalles técnicos de requisitos específicos, consulta `docs/architecture/R*_*.md`
- **Contratos de Módulo**: Para especificaciones a nivel módulo, consulta `docs/contracts/design_by_contract.md`
- **Pruebas**: Para cobertura de pruebas y planes de integración, consulta `docs/testing/`

## Estado de la Especificación

**Activo y Vigente (v0.3.0)**:
- requirements_traceability.md — Especificación maestra normativa
- tech_spec.md — Arquitectura técnica consolidada vigente

**Activo y Complementario (Fase 2)**:
- sdd_edge_cases_phase2.md — Extensiones de casos borde de Fase 2

**Referencia Legacy Archivada**:
- docs/archive/spec_driven_development_legacy_numbering_reference.md — Referencia de numeración legacy R-XX únicamente; no usar para IDs nuevos

**Referencia y Visual**:
- requirements_diagrams_visual.md — Diagramas de R-01 a R-17

## Cobertura de Requisitos

- **R-01 a R-09**: Requisitos core de provisionamiento (particionado, sanitización, backup, distribución)
- **R-10 a R-15**: Infraestructura y gobernanza (sync, checksum, categorías de gobernanza)
- **R-16 a R-17**: Resiliencia (checkpoint, recovery)
- **R-18 a R-22**: Fase 2 (concurrencia, DRM, robustez FAT32, agotamiento de I/O, detección de fraude)
- **R-34 a R-36**: Endurecimiento de seguridad (path traversal, inyección en shell, DoS por bomba de metadatos)

Para más detalle, consulta `requirements_traceability.md`.
