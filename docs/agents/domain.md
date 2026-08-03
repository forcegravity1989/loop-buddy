# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root — the ubiquitous-language glossary (队友=Teammate、环=workflow 内部微循环、时期/就绪状态/发言方等,见词表本体).
- **`docs/adr/`** — read ADRs that touch the area you're about to work in (e.g. `0001-ubiquitous-language.md`).

This is a single-context repo: one `CONTEXT.md` + one `docs/adr/` at the root. No `CONTEXT-MAP.md`.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a spec, a refactor proposal, a test name), use the term as defined in `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0001 (ubiquitous language) — but worth reopening because…_

## Updating (via `/grill-with-docs` → `/domain-modeling`)

- Update `CONTEXT.md` inline the moment a term is resolved — don't batch. Glossary only; zero implementation details.
- Create an ADR only when all three hold: hard to reverse, surprising without context, the result of a real trade-off. Otherwise skip.
