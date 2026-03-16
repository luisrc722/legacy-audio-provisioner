# ADR Index (Immutable)

This folder stores Architecture Decision Records used as immutable historical records.

## Rules

- ADRs are append-only. Do not rewrite accepted ADR bodies.
- If a decision changes, create a new ADR and mark links with `Supersedes` / `Superseded by`.
- Keep ADRs short (context, decision, consequences).
- Land ADR updates in the same commit as the related code/doc change.

## Lifecycle

- `Proposed`: under discussion.
- `Accepted`: active decision.
- `Superseded`: historical record replaced by a newer ADR.

## Naming

Use zero-padded IDs and short slugs:

- `0001-rust-project-structure.md`
- `0002-direct-file-copy.md`
- `0003-ffmpeg-normalization.md`
- `0004-quarantine-isolation.md`
- `0005-sync-sha256.md`

## Template

```md
# ADR 000X: Short Decision Title

- Status: Proposed | Accepted | Superseded by ADR-XXXX
- Date: YYYY-MM-DD
- Author: <name>
- Supersedes: ADR-XXXX (optional)

## 1. Context

Problem and uncertainty being resolved.

## 2. Decision

The rule the system will enforce.

## 3. Consequences

- Positive:
- Negative:
```
