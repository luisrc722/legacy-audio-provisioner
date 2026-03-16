# Changelog

All notable changes to this project are documented in this file.

## [1.1.0] - 2026-03-16

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

### Quality
- Repository now passes strict linting and tests after hardening:
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
- Current test status: `54/54` passing (41 unit + 11 integration + 2 doc).

### Notes
- This release aligns runtime behavior with documented DbC constraints, ADR-0005 sync/hash policy, and legacy architecture notes for atomic checkpointing.
