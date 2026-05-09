# Rubrica Objetiva de Auditoria Tecnica

Version: 1.0

Objetivo: evaluar calidad tecnica de forma reproducible, sin depender de opiniones individuales.

Alcance: aplica a cambios de codigo, arquitectura, trazabilidad, pruebas y documentacion en este repositorio.

## Escala de Puntaje

- Cada criterio vale de 0 a 2 puntos.
- Puntaje total maximo: 20 puntos.

Interpretacion por criterio:

- 0 = No cumple o no existe evidencia.
- 1 = Cumplimiento parcial o evidencia incompleta.
- 2 = Cumplimiento completo con evidencia verificable.

Umbrales de dictamen:

- 18 a 20: Aprobado para release.
- 15 a 17: Aprobado con acciones correctivas menores.
- 12 a 14: No aprobado, requiere remediacion.
- 0 a 11: No aprobado critico.

## Criterios de Auditoria

1. Trazabilidad bidireccional
- Verifica relacion requisito -> implementacion -> prueba -> evidencia.
- Evidencia minima: actualizacion en docs/spec/requirements_traceability.md y casos en docs/testing/integration_tests.md.

2. Determinismo operativo
- Verifica ausencia de identificadores operativos basados en fecha/hora.
- Evidencia minima: nombres/IDs estables en codigo y docs de arquitectura alineadas.

3. Integridad de datos
- Verifica hash, checkpoints atomicos y consistencia de recovery.
- Evidencia minima: pruebas de checkpoint y recovery, mas rutas de error controladas.

4. Seguridad defensiva
- Verifica validaciones contra traversal, inyeccion shell, metadata bomb y rutas fuera de jaula.
- Evidencia minima: pruebas negativas/adversariales en suite de integracion.

5. Resiliencia ante fallos
- Verifica manejo de ENOSPC, read-only filesystem, desconexion y reanudacion segura.
- Evidencia minima: pruebas con errores tipados y comportamiento recovery granular.

6. Calidad de pruebas
- Verifica cobertura funcional, integracion y casos borde.
- Evidencia minima: tests relevantes en verde y mapeados a requisitos.

7. Observabilidad y auditoria
- Verifica logs estructurados, semantica estable y utilidad para post-mortem.
- Evidencia minima: validacion de eventos y campos obligatorios en pruebas.

8. Coherencia codigo-documentacion
- Verifica que docs activas reflejen el comportamiento real del sistema.
- Evidencia minima: sin contradicciones entre README, arquitectura, spec y codigo.

9. Calidad de arquitectura y mantenibilidad
- Verifica separacion de responsabilidades, contratos claros y errores tipados.
- Evidencia minima: entrypoint delgado, orquestador claro y modulos cohesionados.

10. Preparacion de release y gobernanza
- Verifica checklist, changelog y estado de decisiones operativas.
- Evidencia minima: actualizaciones en CHECKLIST.md y CHANGELOG.md cuando aplique.

## Formato de Evaluacion

Plantilla recomendada:

- C1 Trazabilidad: 0/1/2 - evidencia encontrada.
- C2 Determinismo operativo: 0/1/2 - evidencia encontrada.
- C3 Integridad de datos: 0/1/2 - evidencia encontrada.
- C4 Seguridad defensiva: 0/1/2 - evidencia encontrada.
- C5 Resiliencia ante fallos: 0/1/2 - evidencia encontrada.
- C6 Calidad de pruebas: 0/1/2 - evidencia encontrada.
- C7 Observabilidad y auditoria: 0/1/2 - evidencia encontrada.
- C8 Coherencia codigo-documentacion: 0/1/2 - evidencia encontrada.
- C9 Arquitectura y mantenibilidad: 0/1/2 - evidencia encontrada.
- C10 Release y gobernanza: 0/1/2 - evidencia encontrada.

Total: X/20
Dictamen: Aprobado para release | Aprobado con acciones menores | No aprobado | No aprobado critico

## Regla de Bloqueo

Aun con puntaje global aprobatorio, el dictamen pasa a No aprobado si cualquiera de estos criterios queda en 0:

- C3 Integridad de datos
- C4 Seguridad defensiva
- C5 Resiliencia ante fallos

Justificacion: son criterios de riesgo alto para perdida de datos o comportamiento inseguro.
