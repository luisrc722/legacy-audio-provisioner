# Notas de Arquitectura (Detalles de Implementación)

Documentación a nivel de implementación para requisitos (R-XX). Estas notas explican decisiones de diseño y justificación técnica para requisitos específicos del sistema.

**Estado**: Estos archivos son documentación de trabajo, no registros formales de decisión. Para decisiones de arquitectura, consulta `docs/adr/`.

## Notas de Implementación de Requisitos (R-XX)

| Archivo | Requisito | Alcance |
|------|-------------|-------|
| `R04_strict_hardware_validation.md` | R-04 | Validación de dispositivos de hardware para prevenir destrucción del sistema host |
| `R06_audio_normalization_ffmpeg.md` | R-06 | Normalización de formato de audio mediante FFmpeg para garantizar compatibilidad con firmware legacy |
| `R16_R17_checkpoint_and_recovery.md` | R-16, R-17 | Sistema de checkpoint atómico y recovery granular tras interrupciones |
| `R16_sync_before_unmount_and_poweroff.md` | R-16 | Requisitos POSIX de sync antes de desmontar para prevenir corrupción FAT32 |

## Arquitectura General del Sistema

| Archivo | Propósito |
|------|---------|
| `architecture_overview.md` | Diseño de sistema de alto nivel y etapas del pipeline |

## Notas Archivadas/Consolidadas

Consulta `docs/archive/` para notas de implementación sustituidas que han sido consolidadas en los documentos R-XX unificados de arriba. Los archivos marcados con `_superseded_by_*` indican que fueron integrados en versiones consolidadas más nuevas.

## Navegación

- Para implementación de requisitos específicos: consulta los archivos R-XX de arriba
- Para justificación de decisiones: consulta `docs/adr/`
- Para especificación de requisitos: consulta `docs/spec/requirements_traceability.md`
- Para contratos a nivel módulo: consulta `docs/contracts/design_by_contract.md`
