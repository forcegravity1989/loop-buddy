# CodeGraph-first repository navigation

This repository shares its code graph at `.codegraph/codegraph.db`. When the
`codegraph` CLI is available, use it before broad grep/find or opening many
files:

- `codegraph explore "<question or symbol names>"` for architecture, flows, and
  locating the relevant implementation in one call.
- `codegraph node <symbol>`, `codegraph callers <symbol>`,
  `codegraph callees <symbol>`, and `codegraph impact <symbol>` for focused
  follow-up queries.
- Treat source returned by `codegraph explore` or `codegraph node` as already
  read; do not reopen the same file unless additional lines are needed.

Run `codegraph sync` after changing source code so local queries stay current.
Do not add `.codegraph/codegraph.db` to feature commits: pull-request CI verifies
the graph, and the CodeGraph workflow is the single writer that publishes the
shared snapshot after `main` passes CI. Runtime files such as WAL/SHM files,
logs, sockets, and PID files stay machine-local.

Repository-specific product, architecture, verification, and writing rules
remain in `CLAUDE.md` and apply alongside this file. Start at `README.md` for
what the repo is and how to run it; `docs/README.md` is the map of every doc
tree (active docs, runtime assets baked in via `include_str!`, the partner's
`docs/v1~v3-prototype/` iteration line, and `docs/archive/`).
