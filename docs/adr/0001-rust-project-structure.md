# ADR 0001: Rust Project Structure

- Status: Accepted
- Date: 2026-03-16
- Author: Luis / Legacy Audio Project

## 1. Context

The project needs a maintainable structure for long-term evolution, testing, and safety hardening.

## 2. Decision

Adopt a modular Rust architecture where orchestration remains in `src/main.rs` and domain logic is implemented in focused modules (`backup`, `checkpoint`, `diffing`, `distribution`, `hardware`, `normalizer`, `recovery`, `sanitizer`, `verification`, `ipc`).

## 3. Consequences

- Positive:
  - Better isolation of responsibilities and easier testing.
  - Safer incremental hardening without monolithic rewrites.
- Negative:
  - More files and interfaces to keep aligned.
