# ADR 0009: CLI Standardization and Command Orchestration

- Estado: Aceptado
- Date: 2026-03-16
- Author: Luis / Legacy Audio Project

## 1. Contexto
La interfaz de línea de comandos (CLI) original utilizaba flags globales inconsistentes (`--usb-mount`, `--audio-source`). Con la adición de `ingest` y `refactor`, esto genera confusión y aumenta la probabilidad de errores del usuario.

## 2. Decisión
1. **Estandarización:** Se adoptan nombres de flags cortos y consistentes en subcomandos operativos: `--usb` (dispositivo/mount de destino o lectura) y `--source` (origen o staging local según comando).
2. **Orquestación (`refactor`):** Se implementa el subcomando `refactor` que encadena `ingest` + `provision --sync` para simplificar el flujo in-situ.
3. **Breaking Change:** Se eliminan flags legacy en la interfaz principal para forzar una API limpia antes de integración con frontend.

## 3. Consecuencias
- Positivas: Interfaz predecible, facilidad de integración con frontend, reducción de errores operativos.
- Negativas: Incompatibilidad con scripts previos a la v0.2.0.
