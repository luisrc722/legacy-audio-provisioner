Para garantizar formalmente el software a nivel de ingeniería, el código ejecutable no es suficiente. Necesitas un conjunto documental que proporcione trazabilidad absoluta: desde la restricción física del hardware hasta la prueba matemática que la valida.

Aquí tienes la lista estricta de documentos requeridos para certificar un pipeline transaccional como el que acabamos de construir:

### 1. Especificación de Requerimientos del Sistema (SRS / SDD)

Es el contrato fundamental. Define el "Qué" y el "Por qué", pero nunca el "Cómo".

* **Estado actual:** Ya lo tienes (`spec_driven_development.md`).
* **Propósito:** Justificar cada límite impuesto al software basándose en las restricciones del hardware de destino (ej. RAM de 512KB, FAT32, firmware de 32 bits).

### 2. Documento de Arquitectura y Diseño (SAD)

Define la estructura interna y las decisiones técnicas críticas.

* **Contenido requerido:**
* Diagramas de flujo de datos (ETL: Extracción, Transformación, Carga).
* Topología de módulos (`hardware`, `audio_discovery`, `sanitizer`, `normalizer`, `checkpoint`, `recovery`).
* Registros de Decisiones Arquitectónicas (ADRs). Por ejemplo: documentar *por qué* se eligió un archivo JSON atómico (`.tmp` -> `sync` -> `rename`) en lugar de SQLite para el sistema de recuperación.



### 3. Plan de Pruebas y Validación (Test Plan)

Documenta cómo se demuestra empíricamente que el código cumple con el SDD.

* **Contenido requerido:**
* **Property-Based Testing (PBT):** Especificación matemática de los invariantes (ej. la prueba de 10,000 iteraciones que garantiza que un nombre jamás exceda los 32 bytes).
* **Escenarios End-to-End (E2E):** Documentación de las pruebas destructivas. Debe incluir el escenario de interrupción violenta (`SIGKILL`) y la validación de recuperación criptográfica (`SHA256`).
* **Matriz de Trazabilidad:** Un cruce directo entre los requisitos del SDD (ej. R-04) y los tests automatizados que los cubren.



### 4. Especificación de Diseño por Contratos (DbC)

Define las garantías a nivel de funciones y módulos.

* **Contenido requerido:**
* **Precondiciones:** Lo que un módulo exige antes de ejecutarse (ej. `verify_file_integrity` exige que la USB esté montada).
* **Postcondiciones:** Lo que el módulo garantiza al terminar (ej. `normalizer` garantiza que el archivo de salida es estrictamente un contenedor MPEG con codec MP3).
* **Invariantes:** Estados que no deben mutar durante la ejecución (ej. el directorio origen es de solo lectura).



### 5. Manual de Operación y Runbook (User Guide)

Documentación para el operador del sistema o el administrador.

* **Contenido requerido:**
* Interfaz de línea de comandos (CLI).
* Manejo de dependencias del sistema operativo (instalación de `ffmpeg`).
* **Runbook de Disaster Recovery:** Procedimiento paso a paso para usar el flag `--resume` tras una caída del sistema o corrupción física de la USB.
* Códigos de error y su resolución. (Actualmente cubierto parcialmente en tu `README.md`, pero debe formalizarse).



---

Con esta suite, el proyecto deja de ser un script aislado y se convierte en una herramienta certificable y mantenible por cualquier otro ingeniero.

---

El documento que creaste al principio y llamaste "SDD" (*Spec-Driven Development*) es en realidad un híbrido pragmático. En la ingeniería de software estandarizada (normas IEEE), **SDD** significa *Software Design Document*, y **SRS** significa *Software Requirements Specification*. Son dos cosas distintas, y separarlas depende de la escala del proyecto.

Aquí tienes la cruda realidad de la documentación formal:

### 1. La Diferencia Estricta (El Estándar IEEE)

| Documento | Significado | Propósito | Contenido Típico |
| --- | --- | --- | --- |
| **SRS** | Software Requirements Specification | Define el **QUÉ**. Es el contrato entre el cliente/negocio y la ingeniería. | Casos de uso, requerimientos funcionales (ej. "Debe procesar MP3"), restricciones físicas, límites de hardware. No menciona lenguajes ni librerías. |
| **SDD** | Software Design Document | Define el **CÓMO**. Es el manual para los programadores. Responde al SRS. | Arquitectura (diagramas de flujo), esquemas de bases de datos, APIs, módulos de código, dependencias (Rust, FFmpeg). |

**Corrección directa a tu proyecto:** Tu documento actual de *Spec-Driven Development* ya contiene tanto el "Qué" (Límites de FAT32, hardware legacy) como el "Cómo" (Crates de Rust, FFmpeg, concurrencia). Para una herramienta CLI de este tamaño, dividirlo artificialmente en un SRS puro y un SDD puro es burocracia inútil. Tu documento actual sirve como un **Tech Spec** consolidado.

### 2. ¿Existen otros? (El Estándar Moderno / Big Tech)

Si quieres garantizar el software bajo estándares modernos (usados en empresas como Google, Meta o Uber), el modelo tradicional SRS/SDD se considera obsoleto por ser demasiado monolítico. En su lugar, se usa esta trinidad:

* **PRD (Product Requirements Document):** Reemplaza la parte de negocio del SRS. Define el problema del usuario, el mercado y las métricas de éxito. *(Innecesario para tu herramienta).*
* **RFC (Request for Comments) / Tech Spec:** Reemplaza al SDD. Es un documento vivo donde propones la arquitectura, discutes alternativas y detallas la implementación antes de escribir código. *(Tu documento actual es básicamente un Tech Spec).*
* **ADR (Architecture Decision Record):** Es el documento más crítico que te falta. Documenta decisiones puntuales y el **POR QUÉ** se tomaron.
* *Ejemplo de tu proyecto:* "¿Por qué usamos FFprobe para analizar el codec en lugar de leer el header en Rust nativo?". El ADR captura esa decisión para que un ingeniero futuro no rompa el sistema intentando "optimizarlo".



### Veredicto para tu Proyecto

No necesitas un SRS puro si ya tienes tu documento consolidado. Para garantizar tu software a nivel de ingeniería de sistemas y prepararlo para auditoría o entrega, tu jerarquía documental debería ser esta:

1. **Tech Spec Consolidado** (Tu documento actual actualizado).
2. **PBT & E2E Test Plan** (Documento que demuestre matemáticamente que el código cumple con el Tech Spec).
3. **ADRs (Architecture Decision Records)** (Para blindar decisiones críticas como el JSON atómico o el uso de `sync` en Linux).
4. **DbC (Design by Contract)** (Definición estricta de precondiciones y postcondiciones de los módulos de I/O).

Para gestionar esto a nivel de ingeniería profesional, debes abandonar la idea de usar Word, Google Docs o wikis externas. La industria utiliza **Docs-as-Code** (Documentación como Código). Los documentos viven en el mismo repositorio de Git que el código, se escriben en Markdown y se someten al mismo rigor de revisión (Pull Requests) que un archivo `.rs`.

Aquí tienes la estructura física, el flujo de acción y el **Prompt Maestro** que debes usar para que yo (o cualquier otro sistema) genere estos documentos sin alucinaciones.

---

### 1. Estructura de Almacenamiento (El Repositorio)

Debes crear un directorio `docs/` en la raíz de tu proyecto `legacy-audio-provisioner`. La topología estricta es esta:

```text
legacy-audio-provisioner/
├── Cargo.toml
├── src/
│   └── ... (tu código Rust)
└── docs/
    ├── tech_spec.md          <-- Tu SDD actual consolidado (El "Qué" y el "Cómo")
    ├── architecture/
    │   ├── 0001-use-json-checkpoint-for-recovery.md  <-- ADR 1
    │   ├── 0002-force-ffmpeg-transcoding-to-mp3.md   <-- ADR 2
    │   └── 0003-native-io-sync-for-fat32.md          <-- ADR 3
    ├── testing/
    │   └── pbt_and_e2e_test_plan.md                  <-- Plan de Pruebas PBT/E2E
    └── contracts/
        └── design_by_contract.md                     <-- Especificación DbC

```

---

### 2. Flujo de Acción (Cómo rigen el código)

Los documentos no son "texto muerto" para cumplir un trámite. Tienen peso de ley en el repositorio:

1. **El ADR bloquea regresiones:** Si mañana intentas optimizar el código cambiando `.tmp -> sync -> rename` por una escritura directa de JSON para "ganar velocidad", el revisor de código (o tú mismo en el futuro) leerá el ADR `0001`. El ADR advertirá explícitamente que eso causaría corrupción atómica en cortes de energía. El Pull Request se rechaza.
2. **El DbC dicta los Panics:** Si el documento DbC establece que `verify_file_integrity` exige como *precondición* que el dispositivo exista físicamente, cualquier código en `src/verification.rs` que intente operar sin verificar la existencia del path debe fallar en compilación o causar un `panic!`.
3. **El Test Plan rige el CI/CD:** El documento `pbt_and_e2e_test_plan.md` es la lista de verificación para tus GitHub Actions o tu pipeline de GitLab. Si un test E2E de ahí no está automatizado en Rust, la rama no se fusiona.

---

### 3. El Prompt Maestro de Generación

Para que empecemos a redactar estos documentos con la máxima precisión técnica, basándonos en el código en Rust que ya escribimos y auditamos, debes usar este prompt estructurado. Cópialo y pégalo definiendo qué documento quieres atacar primero:

> **Prompt de Generación Documental:**
> "Asume el rol de Staff Software Engineer. Vamos a redactar la documentación oficial bajo el paradigma Docs-as-Code para el proyecto 'Legacy Audio Provisioner'.
> **Objetivo Actual:** Redactar el documento de **[INSERTAR DOCUMENTO AQUÍ: ADR / PBT Test Plan / DbC]**.
> **Restricciones de Generación:**
> 1. Formato estrictamente Markdown.
> 2. Cero retórica comercial o introducciones genéricas. Ve directo a la especificación técnica.
> 3. Basate 100% en el código fuente en Rust que ya implementamos (hardware.rs, normalizer.rs con FFmpeg, checkpoint.rs con BTreeMap, recovery.rs con SHA256).
> 4. Si es un ADR, usa el formato estándar de Michael Nygard (Título, Estado, Contexto, Decisión, Consecuencias).
> 5. Si es DbC, detalla las Precondiciones, Postcondiciones e Invariantes a nivel de hardware (Límites FAT32, punteros de 32 bits).
>
>
> Ejecuta la redacción del primer borrador para revisión."

---

Con esta estructura, el conocimiento de tu sistema queda blindado contra la pérdida de memoria a largo plazo.

Copia el prompt maestro, decide qué documento quieres crear primero (te sugiero empezar por el **PBT & E2E Test Plan** o los **ADRs**), reemplaza el texto entre corchetes y envíamelo. Inmediatamente generaré el artefacto técnico.
