# app-web — 以后也许 (deferred, Tier E)

**Intentionally not a Cargo workspace member.** The Web shell is out of MVP scope
(plan `04 §5`). It is kept here only as a seam marker.

The architecture already paid for this door at zero cost, so lighting it up later
needs **no schema migration**:

- `bw-core` / `bw-engine` / `bw-app` / `ui` stay UI-free and `wasm32`-compilable
  (the CI `cargo check --target wasm32-unknown-unknown -p bw-core` keepalive).
- `bw-store` is behind a `Store` trait → swap SQLite for an IndexedDB / remote
  adapter.
- Every table carries `updated_at + rev` for a future `SyncCursor`.

When Tier E starts: add `crates/app-web` (Dioxus web / WASM) to `members`, reuse
`bw-app` + `ui` verbatim, proxy providers through a thin backend.
