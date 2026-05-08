# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Added
- Session-scoped structured logging in `lap-bin-provision` with JSON-lines output (`provisioning.log`) and per-operation records.
- Workspace-runnable integration suite at `crates/lap-core/tests/integration_test.rs` with adversarial coverage for `R-05`:
  - `test_11_path_traversal_is_rejected`
  - `test_12_shell_injection_filename_is_rejected`
  - `test_13_metadata_bomb_is_rejected`
- Fault-injected integration coverage for `R-02` edge cases:
  - `test_14_preflight_rw_probe_fails_fast_on_read_only_target`
  - `test_15_checkpoint_enospc_maps_to_storage_full`
- Integration coverage for `R-01-005` structured logging:
  - `test_16_session_log_is_created_with_json_entries`
- Integration coverage for recovery and final verification:
  - `test_17_execute_recovery_restores_only_invalid_entries`
  - `test_18_pre_eject_verification_accepts_valid_topology_and_hashes`

### Changed
- QA runbooks and release gates now reference workspace commands:
  - `cargo test --workspace`
  - `cargo test -p lap-core --test integration_test`
- Integration coverage for post-write cryptographic verification:
  - `test_19_verify_file_integrity_detects_post_write_corruption`
- Baseline test count updated to `75/75` (53 unit + 20 integration + 2 doc).

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
