# ADR 0002: Direct File Copy Baseline

- Status: Superseded by ADR-0005
- Date: 2026-03-16
- Author: Luis / Legacy Audio Project

## 1. Context

The initial implementation prioritized delivery speed and started from direct host-to-USB copy semantics.

## 2. Decision

Use straightforward copy behavior as an early baseline while the hardening strategy was still being discovered.

## 3. Consequences

- Positive:
  - Fast initial delivery.
  - Simple execution path.
- Negative:
  - No cryptographic identity for dedup/incremental sync.
  - Full reprocessing overhead on every run.
  - Weak integrity guarantees for large catalogs.
