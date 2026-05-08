# ADR 0001: Rust Project Structure

- Estado: Aceptado
- Fecha: 2026-03-16
- Autor: Luis / Legacy Audio Project

## 1. Contexto

El proyecto necesita una estructura mantenible para evolución a largo plazo, pruebas y endurecimiento de seguridad.

## 2. Decisión

Adoptar una arquitectura modular en Rust donde la orquestación permanezca en `src/main.rs` y la lógica de dominio se implemente en módulos enfocados (`backup`, `checkpoint`, `diffing`, `distribution`, `hardware`, `normalizer`, `recovery`, `sanitizer`, `verification`, `ipc`).

## 3. Consecuencias

- Positivas:
  - Mejor aislamiento de responsabilidades y pruebas más sencillas.
  - Endurecimiento incremental más seguro sin reescrituras monolíticas.
- Negativas:
  - Más archivos e interfaces que mantener alineados.
