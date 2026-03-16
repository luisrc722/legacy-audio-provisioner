# ADR 0004: Backup-First Quarantine for Untracked USB Files

- Status: Accepted
- Date: 2026-03-16
- Author: Luis / Legacy Audio Project

## 1. Context

USB targets may contain untracked customer files not represented in the checkpoint. Deleting unknown files is contract-risky.

## 2. Decision

Before any mutation, backup untracked files to host and then move them to `.legacy_quarantine/<session>/` on USB.

## 3. Consequences

- Positive:
  - Data-loss risk is minimized.
  - USB root is cleaned for deterministic legacy playback.
  - Audit trail is preserved.
- Negative:
  - Temporary storage overhead on host/USB.
  - Slightly longer execution time.
