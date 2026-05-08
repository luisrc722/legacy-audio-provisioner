# ADR 0012: Thin Entrypoint, Orchestrator Layer, and Reporter Abstraction

- Estado: Aceptado
- Date: 2026-03-24
- Author: Luis / Legacy Audio Project
- Sustituye parcialmente a: ADR 0009 (en alcance de estructura del binario)

## 1. Contexto
El binario de provisionamiento acumulaba responsabilidades de parseo CLI, bootstrap, logging, progreso visual, y flujo de negocio. Ese acoplamiento incrementa riesgo de regresiones y dificulta pruebas de evolucion del pipeline.

Al mismo tiempo, el avance de provisionamiento necesitaba soportar salida humana y consumo por frontend sin duplicar logica.

## 2. Decision
1. El entrypoint (`main.rs`) se mantiene delgado: parseo de argumentos, inicializacion de runtime/logging, despacho y mapeo de errores tipados.
2. El flujo de negocio se concentra en `ProvisioningOrchestrator`.
3. El progreso se desacopla mediante el trait `ProgressReporter`, con implementaciones `CliReporter` y `JsonIpcReporter`.
4. El hashing de archivos se centraliza en `lap-core::crypto::compute_file_sha256` para evitar duplicacion entre modulos.

## 3. Consecuencias
- Positivas:
  - mejor separacion de responsabilidades y mantenibilidad,
  - evolucion mas segura de UI/CLI sin tocar logica de negocio,
  - menor riesgo de divergencia funcional al centralizar hashing.
- Negativas:
  - mayor numero de modulos implicados en la ruta critica,
  - incremento inicial en trabajo de trazabilidad y documentacion cruzada.

## 4. Trazabilidad
- R-01-006: EntryPoint Delgada y Orquestacion.
- R-01-007: Abstraccion de Progreso.
- R-06-001: Politica de hashing centralizada.

Anclas de implementacion:
- `crates/lap-bin-provision/src/main.rs`
- `crates/lap-bin-provision/src/orchestrator.rs`
- `crates/lap-bin-provision/src/reporter.rs`
- `crates/lap-core/src/crypto.rs`
