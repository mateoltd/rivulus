# Decisions

## 2026-09-26 — lib `core` renamed to `common`
- `core` as a lib name shadows `::core` (std core) in downstream crates: `thiserror::Error` derive emits `core::fmt` paths that resolve to OUR crate, breaking every downstream derive + doctest.
- Proven with minimal downstream repro (E0433/E0425).
- Fix: dir `crates/core` -> `crates/common`, package `rivulus-core` -> the renamed `common` package (same `rivulus-` prefix), lib `core` -> `common`.
- All code/docs use `common::Error`, `common::json`, `common::validate`, `common::oauth`.
- `09-decisions-locked.md` + `02` updated accordingly (mechanical rename, no policy change).
- Rule: never name a lib `core`, `std`, `alloc`, `test`, `proc_macro`, `tokio`, `serde`.

## 2026-09-26 — P0 WS choice
- Default `tokio-tungstenite 0.24` ships (owns `Message::Text(String)`; accept 1 alloc/msg).
- `tokio-websockets` Bytes evaluation deferred to P3 spike; only one ships.

## 2026-09-26 — P0 TLS
- `reqwest 0.12 + rustls-tls-manual-roots + webpki-roots 0.26` wired in `rest::Client::builder` (P2).

## 2026-09-27 — P10 bench harness: std-only, no criterion
- `criterion 0.8.2` IS in the offline cargo cache, but plans/02+09 are
  frozen and the playbook requires updating 02+09+deny.toml+DECISIONS for
  any new dep. Zero-dep approach instead: `crates/rivulus/benches/` with
  `[[bench]] harness = false` plain `fn main()` binaries (stable-safe).
- Bench-only `CountingAlloc` lives in `benches/replay.rs` only (needs
  `unsafe impl GlobalAlloc`, impossible under lib `#![forbid(unsafe_code)]`).
- Workspace-root `benches/` stays documentary stubs (root has no package,
  never compiled) per the P4/P5 `tests/` mirror precedent.
- Revisiting criterion needs an RFC updating 02+09+deny.toml first.

## 2026-09-27 — D1: TLS on the locked WS stack (P5)
- Single WS stack preserved: `tokio-tungstenite 0.24` stays the only WS
  crate. Added `[features] default = ["tls"]` with
  `tls = ["tokio-tungstenite/rustls-tls-webpki-roots"]` in
  `crates/gateway` (feature name verified in the locked 0.24.0 registry
  manifest). Reuses the already-locked rustls + webpki-roots (P0 TLS
  choice); no new crates, no deny.toml change needed.
- MSRV-safe: workspace is 1.75 and the rest stack already requires rustls.
- `ws://` mocks work with AND without the feature
  (`cargo check -p rivulus-gateway --no-default-features` in CI); live
  `wss://gateway.discord.gg` needs the default-on `tls`.
- Facade pins it: `gateway = ["dep:gateway", "gateway/tls"]` so default
  facade builds keep TLS even if the gateway default ever changes.

## 2026-09-27 — D2: RFC-record for P1a freeze — additive list routes (P5)
- `Route` stays `#[non_exhaustive]`; no variant removed or renamed.
  ADDITIVE only: new variants `ListGuildChannels { guild_id }`,
  `ListMessages { channel_id, limit, after }`,
  `ListCurrentUserGuilds { limit }` (all `Method::Get`, low-cardinality
  `path_template` constants) + additive `Route::query() -> Option<String>`
  (`None` for every pre-existing variant; `limit=N` / `limit=N&after=M`
  for the new list variants that Discord defines params for).
- `ListMembers { guild_id }` deliberately UNCHANGED (no `limit`/`after`
  fields): changing its shape would break the frozen P1a constructor, so
  `query()` is `None` for it and `Members::fetch_page` keeps the
  client-side `take(limit)`.
- Bucket isolation preserved: `Ratelimiter::bucket_key` = method +
  template + major, so `GET /channels/{id}/messages` (list) and
  `POST /channels/{id}/messages` (create) never share a bucket.
- Changing manager fn signatures is allowed (facade managers are NOT
  P1a-frozen): `Channels::fetch_page/stream_pages` now take
  `(guild_id, limit)`.
- Known shape gap (documented, not silently worked around):
  `GET /users/@me/guilds` returns partial guild objects without
  `owner_id`, which `model::Guild` requires (model logic frozen, no new
  types). `Guilds::fetch_page` parses `Vec<model::Guild>` and surfaces a
  truncated `Deserialize` error live rather than caching corrupt entries.

## 2026-09-28 — P11 coverage: measured with cargo-llvm-cov, CI keeps tarpaulin
- Gate substance (>=80% lines on rest/gateway/cache, same denominator:
  other crates excluded, tests included) MEASURED: llvm-cov exit 0 with
  `--fail-under-lines 80`. TOTAL 95.28% lines; cache 98.16%, gateway 94.27%,
  rest 93.51%. Evidence in tarpaulin.toml header comment.
- Tool substitution rationale (environment, not policy): cargo-tarpaulin
  0.37.4/0.37.5 cannot compile here — openssl-sys needs system OpenSSL
  headers (absent, no package manager) or vendored build via openssl-src
  which needs `make` (absent). cargo-geiger 0.13.0 blocked by the same
  openssl-sys wall (unsafe-audit evidence remains `#![forbid(unsafe_code)]`
  + zero `unsafe` blocks by grep). CI `coverage`/`geiger` jobs unchanged.
- New-env toolchain note: no `cc` on the box; builds use a zig-cc shim
  (`zig cc -target x86_64-linux-gnu`, plus `--target=*` filtering and
  `-u <sym>` -> `-Wl,-u,<sym>` translation for coverage linking). No repo
  impact: shim lives outside the repo; no manifest/lockfile changes for it.
