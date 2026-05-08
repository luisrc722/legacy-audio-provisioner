# Flujo de Requisitos

## Regla central

> **La matriz va primero. Siempre.**
>
> Ningún requerimiento existe hasta que tiene un ID `R-CC-NNN` en `docs/spec/requirements_traceability.md`. Los demás documentos **referencian** ese ID; nunca lo definen.

Si escribes un requerimiento en cualquier otro archivo (ADR, nota de arquitectura, spec de casos borde, contrato de módulo, etc.) sin haberlo registrado primero en la matriz, ese requerimiento es invisible para el proyecto.

## Trazabilidad bidireccional obligatoria

La gobernanza profesional del proyecto exige que la trazabilidad funcione en ambos sentidos:

1. **De especificación a código**: toda fila `R-CC-NNN` en `docs/spec/requirements_traceability.md` debe apuntar a un `Implementation Anchor` real.
2. **De código a especificación**: toda función Rust que implemente una garantía normativa debe llevar un comentario con su ID `R-CC-NNN`.
3. **De código a QA**: un requisito solo puede subir a `VERIFIED` si `docs/testing/integration_tests.md` registra un test que lo cubra.
4. **Para Hardware y Seguridad (`02`, `05`)**: `VERIFIED` exige evidencia negativa/adversarial o con fallos inyectados; una ruta nominal no basta.

Plantilla obligatoria para funciones Rust con garantía normativa:

```rust
/// [R-05-001] Path Jail
/// Legacy cross-ref: R-34.
/// Pre-condición: ruta relativa inyectada desde CLI o pipeline interno.
/// Post-condición: la ruta resuelta permanece contenida en el base path.
/// Invariante: ningún componente `..`, absoluto o prefijo de escape puede sobrevivir.
```

Esto no reemplaza `design_by_contract.md`; lo aterriza en el código para auditoría puntual.

---

## Flujo obligatorio al agregar un requerimiento

```
1. Abre docs/spec/requirements_traceability.md
   └─ Elige la categoría correcta (tabla "Mapa de Categorías")
   └─ Asigna el siguiente ID disponible: R-CC-NNN
   └─ Completa: ID │ Legacy ID │ Name │ Technical Description │ Implementation Anchor
   └─ Si el req viene de un legacy R-XX, agrégalo al crosswalk

2. Haz commit de esa línea sola (o incluida en el mismo PR que la feature)

3. Ahora escribe lo que necesites en otros documentos
   └─ ADR: "Este ADR implementa R-05-002 y R-09-003."
   └─ Nota de arquitectura: referencia el ID, no repite la definición
   └─ Spec de casos borde: idem
```

**Nunca al revés.** El flujo inverso (escribir la spec/ADR primero y "luego lo marco en la matriz") es exactamente lo que produjo los 8 requerimientos dispersos que tuvimos que auditar.

---

## Cuándo actualizar la matriz

| Evento | Acción |
|---|---|
| Nueva feature o módulo | Agregar entrada R-CC-NNN antes de abrir el PR |
| Cambio de alcance de un req existente | Editar la fila, NO crear una nueva salvo que el alcance sea genuinamente distinto |
| Cambio de estado (`PROPOSED` → `IMPLEMENTED` → `VERIFIED`) | Actualizar `Requirement Lifecycle Register` en el mismo cambio |
| Req descartado | Marcar como `DEPRECATED` en la columna Name, no borrar (trazabilidad) |
| Nueva fase/milestone | Actualizar el campo `Current baseline` en el encabezado de la matriz |
| Renombrado de archivo fuente | Actualizar el `Implementation Anchor` de todas las filas afectadas |

---

## Cuándo NO crear un nuevo documento markdown

Antes de crear un `.md` nuevo, pregúntate:

1. **¿Puede ir como sección en un documento existente?**
   - Decisión de arquitectura → `docs/adr/`
   - Nota de implementación de un req → `docs/architecture/`
   - Política operativa → `docs/spec/OPERATIONAL_DECISIONS.md`
   - Guía de uso → `docs/guides/usage.md`

2. **¿Es contenido que vive en la matriz o en el contrato del módulo?**
   - Si sí → no crees un archivo nuevo, edita el existente

3. **¿Es un reporte de auditoría o snapshot puntual?**
   - Si sí → va a `docs/archive/` con fechas explícitas en el nombre y no es un documento vivo

Si tras estas preguntas el contenido no encaja en ningún lugar existente, entonces (y solo entonces) crea el archivo nuevo con la ubicación y propósito documentados en el README de esa carpeta.

---

## Versionado de documentación

La documentación **vive** junto al código; evoluciona con cada PR.

- **Nunca pongas la versión del software en el nombre de un archivo vivo** (`requirements_traceability_v0_3_0.md` es incorrecto).
- El historial de cambios está en `git log`; no necesitas copiar el archivo para "preservar" una versión anterior.
- Si el documento necesita mostrar su estado actual, usa un **encabezado interno**:
  ```markdown
  | Current baseline | v0.3.0 |
  | Last updated     | 2026-03-17 |
  ```
- Los **snapshots de auditoría** (PDFs, exports de release) sí pueden llevar versión en el nombre porque son inmutables por diseño.

---

## IA para documentación técnica (estilo Google)

Cuando uses IA para redactar ADRs o specs, no uses prompts libres. Usa la plantilla oficial:

- `docs/guides/ai_master_prompt_google_style.md`

Reglas mínimas de cumplimiento:

1. No se aceptan textos con retórica o afirmaciones no verificables.
2. Toda garantía nueva debe mapear a `R-CC-NNN` en `requirements_traceability.md`.
3. El output debe incluir invariantes y no-objetivos.
4. Para `R-02` y `R-05`, el plan de QA debe contemplar evidencia adversarial/con fallos inyectados.

---

## Flujo formal para generar una spec ejecutable

1. **Identificación**: define la necesidad en lenguaje operativo.
2. **Categorización**: asigna `R-CC-NNN` en la matriz antes de tocar código.
3. **Invariante**: redacta explícitamente qué propiedad no puede romperse.
4. **Contrato**: si el req altera pre/post-condiciones de un módulo, actualiza `docs/contracts/design_by_contract.md`.
5. **ADR**: si cambia arquitectura, límites de despliegue o responsabilidades entre crates, documenta la decisión en `docs/adr/`.
6. **Código**: implementa la función con comentario `/// [R-CC-NNN] ...`.
7. **QA**: agrega o actualiza el test y registra la evidencia en `docs/testing/integration_tests.md`.
8. **Estado**: solo entonces mueve el req a `VERIFIED`.

Si el requisito pertenece a `02` o `05`, el paso 7 debe incluir explícitamente un caso de fallo, input hostil o condición degradada.

Ejemplo correcto con IDs actuales del repositorio:

- Prevención de extracción insegura / cierre transaccional: `R-09-010`
- Bloqueo de path traversal: `R-05-001`
- Límite físico de 50 archivos por volumen: `R-08-001`

No inventes IDs en texto libre. Si un ejemplo necesita un ID nuevo, se asigna en la matriz primero.

---

## Gobernanza del workspace

La separación por crates no es solo organizativa; es parte del modelo de auditoría:

- `lap-core`: lógica auditable pura, invariantes, seguridad, hashing, recuperación.
- `lap-bin-ingest`: frontera de lectura y validación de entrada.
- `lap-bin-provision`: frontera de escritura física, checkpoint y cierre transaccional.

Si una feature nueva mueve responsabilidades entre crates, eso requiere:

1. Actualizar `requirements_traceability.md`.
2. Revisar `design_by_contract.md`.
3. Evaluar si corresponde ADR.

---

## Estructura de documentos permitida

```
docs/
├── adr/            ← Decisiones arquitectónicas (inmutables, con supersession)
├── architecture/   ← Notas de implementación de reqs específicos (R-XX_nombre.md)
├── spec/           ← Specs normativas vivas (requirements_traceability.md, OPERATIONAL_DECISIONS.md, …)
├── guides/         ← Guías operativas (usage.md, requirements_workflow.md)
├── contracts/      ← Contratos de módulo (design_by_contract.md)
├── testing/        ← Plan de pruebas
└── archive/        ← Solo documentos supersedidos o snapshots de auditoría
```

Antes de agregar una carpeta nueva, discútelo en un ADR.

---

## Checklist rapido antes de cada PR que toca requerimientos

- [ ] ¿El req tiene ID `R-CC-NNN` en `requirements_traceability.md`?
- [ ] ¿El estado del req (`PROPOSED`, `IMPLEMENTED`, `VERIFIED`, `DEPRECATED`) fue actualizado en el `Requirement Lifecycle Register`?
- [ ] ¿El `Implementation Anchor` apunta al archivo de código correcto?
- [ ] ¿La función Rust relevante tiene comentario `/// [R-CC-NNN]` con pre/post/invariante?
- [ ] ¿Si viene de legacy, el crosswalk está actualizado?
- [ ] ¿`docs/testing/integration_tests.md` registra la evidencia si el req se marcó como `VERIFIED`?
- [ ] Si el req es de `02` o `05`, ¿la evidencia incluye fallo inyectado, input hostil o escenario adversarial?
- [ ] ¿El `Current baseline` en el encabezado de la matriz sigue siendo correcto?
- [ ] ¿Creé algún documento nuevo innecesariamente? (si sí, muévelo a su lugar correcto)
