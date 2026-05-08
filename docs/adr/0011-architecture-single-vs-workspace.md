# ARCHITECTURAL DECISION RECORD (ADR) 0011

**Title:** Decisión de Arquitectura Single-Crate vs Workspace
**Date:** 17 de Marzo, 2026
**Status:** SUSTITUIDO POR ADOPCIÓN DE WORKSPACE v0.3.0
**Scope:** Estructura y Organización del Proyecto

---

## Planteamiento del Problema

A medida que el proyecto creció desde prototipo (R-01 a R-33) hacia producción, surgieron preguntas:

- ¿Debemos dividir `main.rs` en múltiples ejecutables?
- ¿Binaries separados mejorarían la velocidad de compilación?
- ¿La modularización reduciría los tiempos de build para desarrolladores?
- ¿Un Rust Workspace es la abstracción correcta?

**Causa Raíz de la Pregunta:** Confundir modularización = múltiples binaries. No son lo mismo.

---

## Contexto

### Lo que tenemos
```
legacy-audio-provisioner/
├── src/lib.rs                    (15 public modules)
├── src/main.rs                   (CLI orchestrator using lib.rs)
├── tests/                        (Integration tests)
└── Cargo.toml                    (Single package, ~50 dependencies)
```

### Métricas de Escala
- **Líneas Totales de Código:** ~5,000
- **Módulos Core:** 15 (alta cohesión)
- **Tiempo de Build:** 3-4 minutos (release)
- **Tamaño del Binario:** 8 MB (stripped, release)
- **Cantidad de Pruebas:** 53
- **Desarrolladores Activos:** 1-2

---

## Decisión Histórica

Este ADR capturó la justificación pre-migración para mantener un único crate mientras el codebase era más pequeño. Sigue siendo útil como contexto histórico de los tradeoffs considerados antes de `v0.3.0`.

Desde `v0.3.0`, el repositorio migró a un Rust Workspace activo con `lap-core`, `lap-bin-ingest`, `lap-bin-provision` y `lap-cli-tools`.

Ver `docs/MODULAR_ARCHITECTURE_ROADMAP.md` para la arquitectura operativa actual.

### Por qué gana Single Crate

#### 1. Compilación
- ✅ Dependencias compiladas una sola vez, usadas por lógica core y main
- ✅ Compilación incremental (Rust toca solo módulos cambiados)
- ❌ Multi-crate: cada crate compila dependencias de forma independiente (sobrecosto 3x)

**Ejemplo:**
```
Single Crate:
  cargo build --release
  → Compiling serde (1 time) → Used by lib + main
  → Total: 3m 00s

Workspace (today):
  cargo build --release
  → Compiling serde (5 times, once per crate)
  → Total: 8m 30s
```

#### 2. Seguridad de Tipos
- ✅ El compilador de Rust fuerza contratos de API en todo el codebase
- ✅ Refactorizar una firma de función dispara recompilación de todos los callers
- ❌ Multi-crate: los binaries pueden quedar desincronizados (requiere coordinación cuidadosa de releases)

**Example:**
```rust
// Today: Change in diffing.rs signature
fn calculate_sync_diff(audio: &AudioFile) -> Result<SyncDiff>  // Changed

// Rust immediately fails all callers:
error[E0308]: expected `&AudioFile`, found `&IngestedFile`
  → All 5 call sites in main.rs must be updated
  → Compiler ensures correctness

// With Workspace: If you don't recompile lap-provision,
// it could be using old API → Runtime panic ⚠️
```

#### 3. Redundancia de Dependencias
- ✅ Single build: serde, sha2, chrono, etc. compiled once
- ✅ Binary size: 8 MB (tightly packed)
- ❌ Workspace: Each binary packages its own copy
  - lap-core: 2 MB (core logic)
  - lap-cli: 5 MB (includes serde, clap, sha2, etc.)
  - lap-ingest: 5 MB (duplicates serde, sha2, etc.)
  - lap-provision: 5 MB (duplicates again)
  - Total: 17 MB (2.1x bloat!)

#### 4. Complejidad de IPC Evitada
- ✅ Today: All calls in-process, instant
- ✅ Core logic directly accessible to CLI
- ✅ No serialization overhead
- ❌ Workspace: Would need:
  - JSON Lines protocol parsing
  - Error propagation between processes
  - Handling of crashed child processes
  - State reconstruction on partial failure

---

## Cuándo Reconsiderar: Los Cuatro Disparadores

### Disparador 1: Explosión de Tamaño de Código
| Métrica | Umbral | Acción |
|--------|-----------|--------|
| LOC | > 50,000 | Re-evaluate |
| Modules | > 30 | Re-evaluate |
| Build Time | > 10 min | Re-evaluate |

**Current:** 5,000 LOC, 15 modules, 3m 00s → No trigger

### Disparador 2: Crecimiento de Equipo
| Tamaño de equipo | Organización |
|-----------|--------------|
| 1-2 devs | Single crate (current) |
| 3-4 devs | Still single crate, better code reviews |
| 5+ devs | Consider Workspace (separate teams per component) |
| 10+ devs | Definitely Workspace (separate deployment pipelines) |

**Current:** 1-2 devs → No trigger

### Disparador 3: Necesidad de Separar Binaries
| Escenario | Disparador | Ejemplo |
|----------|---------|---------|
| Run on same machine | No trigger | Single binary fine |
| Different deployment targets | Possible trigger | Ingest on server A, provision on server B |
| Resource constraints | Possible trigger | Embedded system with 10 MB RAM |

**Current:** Single deployment → No trigger

### Disparador 4: Performance se Convierte en Cuello de Botella
| Síntoma | Severidad |
|---------|----------|
| Rebuilding takes 30 sec for CLI change | Annoying |
| Rebuilding takes 2 min for CLI change | Warning sign |
| Rebuilding takes 5+ min for CLI change | Action needed |

**Current:** CLI change triggers ~1m incremental rebuild → Acceptable

---

## Razonamiento Incorrecto que **NO** Seguimos

### ❌ "Modules = Separate Binaries"
**Error común:** Para tener buena modularidad necesitas crates/binaries separados.

**Realidad:** El sistema de módulos de Rust (`pub mod`) provee excelente separación de responsabilidades SIN múltiples binaries.

We already have it:
```rust
// src/lib.rs
pub mod ingestion;        // 300 lines, can be developed independently
pub mod normalizer;       // 400 lines, orthogonal to ingestion
pub mod verification;     // 250 lines, independent responsibility
pub mod security;         // 380 lines, cross-cutting concern
// All built together, tested together, deployed together
```

### ❌ "Single Crate = Monolito"
**Misconception:** Single build target means monolithic architecture.

**Truth:** Monolith = tight coupling; Single Crate = can be highly modular.

We're NOT a monolith:
```
✅ Each module has clear responsibility
✅ Modules communicate via well-defined APIs (Result types)
✅ Internal implementation changes don't affect other modules
✅ Could easily extract a module to separate crate later if needed
```

### ❌ "Workspace resuelve velocidad de compilación"
**Misconception:** Workspace = automatically faster builds.

**Truth:** Workspace helps ONLY if:
- You never need to rebuild everything
- Teams work on isolated components
- Deployment doesn't require all components

**For our use case:** We always deploy all components together → Workspace doesn't help much.

---

## Ruta de Escalamiento (si se activan los disparadores)

### Etapa 1: Single Crate (HOY) ✅
```
legacy-audio-provisioner/
├── src/lib.rs (15 modules, all public)
├── src/main.rs (CLI using lib APIs)
└── Cargo.toml
```
**Estilo:** Límites limpios entre módulos, compilación compartida.

### Etapa 2: Reorganización de Módulos (SI se necesita en 30k LOC)
```
legacy-audio-provisioner/
├── src/lib.rs
├── src/
│   ├── core/         (journal, diffing, distribution)
│   ├── io/           (ingestion, verification, backup)
│   ├── effects/      (normalizer, hardware, recovery)
│   └── policy/       (security, sanitizer, error)
├── src/main.rs
└── Cargo.toml
```
**Sigue siendo single crate.** Solo módulos mejor organizados.

### Etapa 3: Workspace (SOLO si hay escala + crecimiento del equipo)
```
legacy-audio-workspace/
├── crates/
│   ├── lap-core/     (library with core logic)
│   ├── lap-cli/      (main orchestrator)
│   ├── lap-tools/    (if we need specialized utils)
└── Cargo.toml (workspace)
```
**Only when all four triggers align.**

---

## Evaluación de Riesgo: Single Crate

| Riesgo | Probabilidad | Mitigación |
|------|------------|-----------|
| **Code becomes hard to navigate** | Low | Good module organization + docs |
| **Build times become unacceptable** | Low (at 5k LOC) | Will take 30k+ LOC to hit limit |
| **Team coordination issues** | Low (1-2 devs) | Good git practices + PR reviews |
| **Can't decouple cores later** | Very low | Rust modules → crates migration is straightforward |

---

## Evaluación de Riesgo: Workspace (si se adopta prematuramente)

| Riesgo | Probabilidad | Impacto | Hoy |
|------|------------|--------|-------|
| **IPC desynchronization bugs** | High | Critical | AVOID |
| **Binary dependency bloat** | High | +10 MB+ | AVOID |
| **Increased build complexity** | High | Harder debugging | AVOID |
| **Version mismatch crashes** | Medium | Runtime failures | AVOID |
| **Harder integration testing** | High | Incomplete coverage | AVOID |

**Verdict:** Zero benefit at current scale. All costs, no gains.

---

## Enfoque Aprobado: Desarrollo Module-First

Incluso dentro de single crate, aplicamos disciplina de Workspace:

**Regla 1: Límites fuertes de API**
```rust
// Each module exports Result<PublicType>
// Never exposes internal implementation details

pub mod journal {
    pub struct MoveTransaction { ... }  // Public type
    pub fn mark_committed(...) -> Result<()>  // Public API
    // impl details are private
}
```

**Regla 2: Probar cada módulo de forma independiente**
```rust
// tests/journal_integration.rs
// tests/diffing_integration.rs
// tests/security_scenarios.rs
// Each tests its module in isolation
```

**Regla 3: Grafo de dependencias claro**
```
main.rs
  ├── ingestion
  ├── normalizer
  ├── provision
  │   ├── diffing
  │   ├── journal
  │   └── security
  ├── verification
  └── hardware

No circular dependencies.
Each module depends only on lower-level ones.
```

---

## Checklist de Decisión

- ✅ Current architecture is correct for current scale
- ✅ Single Crate with module-first approach is SOLID
- ✅ Compilation times are acceptable
- ✅ Type safety is maintained across entire system
- ✅ No IPC overhead or complexity
- ✅ Easy to test and refactor
- ✅ Future migration to Workspace is straightforward
- ❌ Do NOT implement Workspace today (premature optimization)

---

## Si alguna vez necesitamos migrar

We've documented the full migration path in [MODULAR_ARCHITECTURE_ROADMAP.md](MODULAR_ARCHITECTURE_ROADMAP.md):
- IPC protocol (JSON Lines)
- Component responsibilities
- Trigger conditions
- Step-by-step migration checklist

**But we don't do this today.**

---

## Conclusión

**Single-Crate Architecture with Strong Module Boundaries = Professional Engineering Decision**

This is not a limitation. This is a feature. We get all the benefits of modularity without the complications of IPC, binary redundancy, or version mismatches.

**Keep it simple. The simple architecture is the strong one.**

---

**Aprobación:** Comité de Arquitectura
**Fecha de Decisión:** 17 de Marzo, 2026
**Fecha de Revisión:** Cuando se cumpla cualquier condición disparadora (50k LOC, 5+ miembros de equipo, etc.)
**Status:** ✅ FINAL
