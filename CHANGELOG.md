# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Changed
- Inversion del pipeline de proteccion de nombres: primero sanitizacion y luego validacion de seguridad, evitando falsos positivos sobre nombres sin normalizar.
- Transicion de carga masiva a evaluacion streaming en diff incremental para reducir presion de RAM y mejorar progreso en tiempo real.
- Sanitizacion inteligente reforzada: transliteracion ASCII, poda de junk inicial/final por regex y normalizacion de separadores a `_`.
- Motor de sincronizacion idempotente por hash: deteccion por hash8 en nombre legacy para `SKIP` de contenido existente aunque cambie carpeta/indice.
- Escalabilidad de indexador: escritura con prefijo de 4 digitos (`{:04}`), continuidad por high-water mark y compatibilidad de lectura 3/4 digitos en transicion.
- Topologia de volumenes `VOL_XX` mantenida con indice global como orden canonico de reproduccion para firmware legacy.

## [0.4.0] - 2026-05-09

### Added
- Preflight `--strict-parity` para provision incremental, con validacion source -> manifest y manifest -> USB antes de mutar contenido.
- Cobertura operativa en guia para escenarios host -> USB, USB vacia/nueva y USB ya procesada con host mixto.
- Guia rapida de idioma runtime (`--lang`, `LAP_LANG`) con precedencia explicita.

### Changed
- Logging estable por dispositivo/operacion:
  - USB especifica: `~/.lap/logs/device_<slug_hash>.log`
  - Operaciones sin USB (`list`): `~/.lap/logs/op_<slug_hash>.log`
- Session IDs deterministicas basadas en identidad estable en lugar de timestamp+PID.
- `timestamp` de evento solo en `SESSION_START` para reducir ruido en diffs y auditoria.

### Fixed
- Reduccion de ruido temporal en logs estructurados por ejecucion.
- Higiene del repositorio: `.obsidian/` agregado a `.gitignore` para excluir metadatos locales.

## [0.3.0] - 2026-03-16

### Fixed
- Enforced strict `--dry-run` semantics with zero writes to USB and local disk.
- Removed non-UTF8 panic path in audio analysis by replacing `to_str().unwrap()` with lossy path conversion before `ffprobe`.
- Corrected checkpoint timestamp persistence so `last_updated` is serialized with the current value.

### Security
- Hardened checkpoint durability for power-loss scenarios: after atomic `rename`, the parent directory is synced (`sync_all`) to persist directory entry metadata.
- Enforced zero-trust hash validation: missing or invalid SHA256 values are treated as verification failures (no silent bypass).
- Recovery now marks malformed or missing checkpoint hashes as candidates for reprocessing/re-normalization.

### Changed
- Verification policy now fails closed on cryptographic anomalies instead of continuing permissively.
- Module header documentation style cleaned to satisfy strict lint pipelines (`clippy -D warnings`).

### Documentation
- Established canonical Docs-as-Code governance via ADR-0006 (`docs/adr/0006-docs-as-code-governance.md`).
- Updated core documentation with visual Mermaid flows:
  - Release gates in `CHECKLIST.md`
  - Provision/recovery pipeline in `docs/spec/tech_spec.md`
  - Integration traceability in `docs/testing/integration_tests.md`
- Clarified source-of-truth boundaries:
  - Canonical ADRs in `docs/adr/`
  - Legacy context in `docs/architecture/` and `docs/archive/`
- Hardened release process with two physical-risk gates:
  - eject handshake verification before physical removal
  - quarantine quota check (`.legacy_quarantine` <= 10% USB capacity)

### Quality
- Repository now passes strict linting and tests after hardening:
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
- Current test status: `54/54` passing (41 unit + 11 integration + 2 doc).

### Notes
- This release aligns runtime behavior with documented DbC constraints, ADR-0005 sync/hash policy, and legacy architecture notes for atomic checkpointing.
