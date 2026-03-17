# ADR 0007: Canonical Path Validation and Circularity Prevention

- **Status:** Accepted
- **Date:** 2026-03-16
- **Requirement:** R-30

## 1. Context

The CLI accepts `--usb-mount` and `--audio-source` as independent paths. There is a high risk of circularity if a user accidentally points both arguments to the same device, or if `audio-source` is a subdirectory inside `usb-mount` (e.g., the quarantine folder `.legacy_quarantine` or a previously created `VOL_XX` directory).

This would cause the engine to read files it just wrote, overwrite originals during normalization, or enter an infinite scan loop consuming disk until the device is full.

## 2. Decision

Implement a strict canonical path validation layer (`validate_canonical_paths`) invoked **before any I/O** in the `provision` command:

1. **Resolution via `canonicalize()`:** Both paths are resolved to their real filesystem identity, eliminating symlinks, `./`, `../`, and hardware aliases before comparison.
2. **Equality block:** If `usb_canonical == source_canonical`, abort with `ProvisioningError::InvalidConfig`.
3. **Nesting block:** If `source_canonical.starts_with(usb_canonical)`, abort with `ProvisioningError::InvalidConfig`. This prevents treating the engine's own output (`VOL_XX`, `.legacy_quarantine`) as new source audio.
4. **Error mapping:** Failures surface as `INVALID_CONFIG` in the IPC JSON stream and in the human-readable error message.

This validation is intentionally **not** applied to `resume`, since that command reads from a host backup directory (`$HOME/usb_backup_*`) — a path structurally independent from `usb-mount`.

## 3. Consequences

**Positive:**
- Absolute protection against data corruption from circular processing.
- Deterministic path resolution regardless of how the user types the path (relative, absolute, symlink).
- Clear, actionable error message at CLI entry point, before any device is touched.

**Negative:**
- `canonicalize()` requires both paths to exist on disk at validation time. Paths that do not yet exist cannot be pre-validated.

## 4. Relation to Other ADRs

| ADR | Connection |
| :--- | :--- |
| ADR-0004 Quarantine Isolation | R-30 ensures quarantine dirs are never re-ingested as source |
| ADR-0005 Sync SHA256 | Canonical paths guarantee hashes are compared for the same physical device |
| ADR-0006 Docs-as-Code | This decision is documented here and referenced in tech_spec.md |
