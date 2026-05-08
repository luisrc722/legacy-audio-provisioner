## Summary
[Clear, technical description of the change]

## Requirement Traceability (SSOT)
- [ ] **Matrix updated:** I added/updated the requirement entry in `docs/spec/requirements_traceability.md`.
- [ ] **Primary ID:** `R-CC-NNN` (specify main impacted requirement).
- [ ] **Legacy Cross-ref:** If applicable, I documented the related legacy `R-XX` in the crosswalk.

## Implementation Evidence (Anchors)
- [ ] **Code anchor present:** I added/updated `/// [R-CC-NNN]` with preconditions, postconditions, and invariants.
- [ ] **Anchor location(s):** `crates/lap-.../src/...rs` (list exact paths/functions).

## Validation and QA
- [ ] **Integration evidence synced:** I updated `docs/testing/integration_tests.md` with the test-to-requirement mapping.
- [ ] **Baseline check:** Existing suite passes (`cargo test`) or updated expected baseline is documented.

## Hardening Gate (Categories 02 and 05)
If this PR affects Hardware (`R-02-*`) or Security (`R-05-*`), adversarial evidence is mandatory.

- [ ] **Fault injection/adversarial evidence attached:** e.g., path traversal blocked, read-only FS, ENOSPC, injected malformed input.
- [ ] **Typed failure observed:** logs/output show expected typed failure path and safe rollback semantics.

## Governance Checklist
- [ ] ADR impact evaluated; ADR added/updated if architectural boundaries changed.
- [ ] If AI assisted documentation was used, output followed `docs/guides/ai_master_prompt_google_style.md`.
- [ ] No out-of-structure `.md` files were added outside governed locations.
- [ ] `scripts/traceability_lint.sh` passes locally.

## Traceability Lint Output
Paste output (or key lines) from:

```bash
bash scripts/traceability_lint.sh
```
