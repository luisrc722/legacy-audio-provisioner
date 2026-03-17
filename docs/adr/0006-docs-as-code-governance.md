# ADR 0006: Docs-as-Code Governance and Canonical Sources

- Status: Accepted
- Date: 2026-03-16
- Author: Luis / Legacy Audio Project

## 1. Context

The project grew from a single README into multiple technical modules (sync, quarantine, typed errors, checkpoint recovery). Without explicit governance, documentation drift appears quickly and causes operational mistakes.

## 2. Decision

Adopt an explicit Docs-as-Code governance model with canonical sources:

1. Architectural decisions are canonical only in `docs/adr/`.
2. `docs/tech_spec.md` is the implementation-level source of truth.
3. `docs/testing/` stores validation evidence and baseline counts.
4. `docs/architecture/` and `docs/archive/` are legacy/historical context, not normative.
5. Any functional code change must include corresponding doc updates in the same commit series.

## 3. Consequences

- Positive:
  - Clear source hierarchy reduces ambiguity during audits and reviews.
  - Faster onboarding because readers know where to trust first.
  - Lower risk of release mistakes caused by stale claims.
- Negative:
  - Slight overhead per feature to keep docs synchronized.
  - Requires discipline in PR/commit hygiene.
