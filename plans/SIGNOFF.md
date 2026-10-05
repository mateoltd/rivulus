# SIGNOFF — 1.0 readiness (P12 terminal, voice excluded)
#
# TERMINAL STATE 2026-09-28: every autonomously-closable box is closed.
# Remaining items are structurally owner-side (no git repo => no tags;
# no crates.io credentials => no publish; live token/guilds => soak-72h,
# live tests, djs compare, dogfood manual). See "Explicit NOT GREEN / MANUAL".

Version: `0.1.0` (unreleased, pre-1.0 `0.x`). API **not yet frozen**.
Voice = stub only (deferred post-1.0, D6) and excluded from every gate below.
Refreshed: 2026-09-28. Toolchain used for evidence: `cargo 1.98.1` (stable),
`CARGO_TARGET_DIR=.tmp/target`, `CC`=zig-cc shim (no system cc on box).
MSRV 1.75 itself is CI-matrix-covered, not locally installed here.

## S1 Parity — PARTIAL (code-complete, live/manual parts open)

- Workspace suites: `cargo test --workspace --all-features` → **31 suites,
  328 passed, 0 failed** (unit + integration + doctests, owner-measured
  2026-09-28). Includes `soak_short`, `dispatcher_lag`, `cache_diff`,
  `collectors`, `ratelimit_conformance` + `rest_extra` (16),
  `verify_vectors`, `roundtrip` (125), `gateway_resume` (4) +
  `gateway_extra` (29). **78 files** under `tests/fixtures/`, incl.
  `components_v2.json`.
- Examples: all six build AND **run mock-safe offline, exit 0**
  (owner-measured 2026-09-28): `ping` (fetch_page offline path handled),
  `slash` (registered: 1), `buttons-collector` (end_reason MessageDelete),
  `sharded` (placeholder shard), `cache-tuning` (hit_ratio 1.00),
  `webhook-verify-axum` (hardening ok).
- Live `#[ignore]` tests (`DISCORD_TOKEN`+`GUILD_ID`): **MANUAL, not run**.
- Mocks `tests/support/mock_gateway.rs` (scripted WS) + `mock_rest.rs`
  present (P1a gate).

## S2 Memory — PARTIAL (rivulus side measured twice, djs compare structural-manual)

- Rivulus benches `crates/rivulus/benches/{replay,rest_ratelimit}.rs`
  (std-only, `harness=false`, bench-only `CountingAlloc`), `bench/compare.py`
  (tolerance bands, warn-only), `bench/djs-equiv/` skeleton.
- 2026-09-27: replay worst p99 **0.0010ms** PASS all 12 cases, 0–5 allocs/ev,
  0–260 B/ev; REST available p99 0.374ms, wait honored, shared excluded.
- 2026-09-28 re-run (new OS/toolchain): replay worst p99 **0.00075ms** PASS;
  rest PASS; `compare.py` exit 0 with 2 sub-µs WARNs logged as timer noise
  in `bench/results/2026-09-28.md` (+ JSON artifacts). `--out` must be
  absolute (bench CWD is `crates/rivulus/`) — documented in that file.
- RSS vs discord.js: NOT measured — `bench/djs-equiv/bot.js` needs a live
  token AND 100–50k guild memberships; unmeasurable in this loop by the
  standing no-tokens rule. RAM table stays template on the djs side.

## S3 Correctness — PARTIAL (mocks green, long soak manual)

- Mock-WS resume (drop→resume, 4007-fresh, latency) / close-code matrix /
  429 conformance suites green (part of the 31). Short chaos `soak_short` green.
- `soak-72h`: **MANUAL (not run)** — `#[ignore]`, needs live infra + 72h.
- Shared-scope 429 exclusion + pre-emptive waits asserted via mock headers
  (P2 conformance suite green).

## S4 Ergonomics — CODE EVIDENCE ONLY (dogfood session manual)

- `examples/ping.rs` builds sync (`builder/add_handler/build`), registers a
  struct handler **and** a closure handler, and exercises dual pagination
  (`fetch_page` + `stream_pages`); ran offline exit 0 (see S1).
- Fresh-agent `<30min` dogfood without reading internals: **not evaluated**.

## S5 Reliability — GREEN (code-measurable parts; soak-72h manual)

- `cargo fmt --check`, `clippy --workspace --all-features -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`,
  31 suites / 328 passed / 0 failed — all owner-measured 2026-09-28.
- `cargo deny check` green (deny 0.20.2, 2026-09-28: advisories/bans/licenses/sources ok).
  Earlier P11 fix retained: `Unicode-3.0` (ICU via reqwest→idna) and
  `CDLA-Permissive-2.0` (webpki-roots, locked P0 TLS) in `deny.toml`.
- `cargo audit` green 2026-09-28 (audit 0.22.2, fresh 1273-advisory DB:
  0 vulnerabilities, 215 deps). CI `audit` job is `continue-on-error`
  (advisory DB needs network) — informational only.
- `cargo geiger`: **uninstallable here** (0.13.0 needs openssl-sys: no system
  headers, no `make`). CI `geiger` job retained. Green-by-construction:
  `#![forbid(unsafe_code)]` workspace-wide, zero `unsafe` blocks by grep.
- `tarpaulin >=80%` (rest/gateway/cache): **MEASURED 2026-09-28 — PASS**.
  cargo-tarpaulin itself is uninstallable in this env (openssl-sys: no system
  headers, no `make`, no package manager), so measurement used cargo-llvm-cov
  0.9.1 with the identical denominator policy (other crates excluded, tests
  included): exit 0 vs `--fail-under-lines 80`. Lines TOTAL **95.28%**
  (3772/3959); per-package **cache 98.16%, gateway 94.27%, rest 93.51%**.
  Rationale + toolchain notes in `plans/DECISIONS.md`; `tarpaulin.toml` header
  holds the exact command. CI `coverage` job (tarpaulin) unchanged for
  tool-fidelity. `cargo-geiger` likewise uninstallable here (same openssl
  wall); unsafe-audit evidence is `#![forbid(unsafe_code)]` workspace-wide +
  zero `unsafe` blocks by grep.
- `cargo check -p rivulus --no-default-features` green (new CI `no-default`).
- 0 panics across the 31 suites. `shutdown_handle()`/5s-drain per P8
  implementation; real `login()` via RestBootstrap (P5 D-decisions).

## Release (P11) — DRY-RUN EVIDENCE (re-verified 2026-09-28)

- `cargo package --workspace --exclude xtask --no-verify`: **0 warnings**.
- `cargo publish --dry-run --no-verify -p rivulus-common` (leaf): **ok**
  ("aborting upload due to dry run" — the success marker).
- `cargo publish --dry-run --no-verify` on package rivulus-common (the leaf): **ok**.
- The other 9 crates fail dry-run with `no matching package named ...` for the
  unpublished intra-workspace deps — **expected pre-1.0 ordering**, not a
  packaging bug: real publish must go bottom-up
  (common → model → gateway/rest/cache → … → rivulus).
  CI `publish-dry-run` packages all + dry-runs the leaf, with this note.
- `CHANGELOG.md` has `## [0.1.0] — Unreleased` (CI `changelog` job greps it).
- **Not done**: API freeze, `rivulus-vX.Y.Z` tags, crates.io publish.

## CI (13 jobs) — config green, runners not executed here

`fmt`, `clippy`, `build-test` (linux/win/mac × 1.75/stable), `doc`,
`lint-guards`, `deny`, `geiger`, `audit`†, `bench-quick`†, `no-default`,
`publish-dry-run`, `coverage`†, `changelog` († = `continue-on-error`).
YAML validated (`yaml.safe_load`, 13 jobs). P11 CI fixes: `doc` job now uses
`RUSTDOCFLAGS` (`cargo doc` has no `--` passthrough — old form errored
`unexpected argument '-D'`); stutter-guard reworded 3 meta-mentions in
`plans/README.md`, `plans/DECISIONS.md`, `plans/09-decisions-locked.md`
(meaning unchanged; guard now passes verbatim).

## xtask — done, over advisory budget

`xtask/src/main.rs` (83 lines post-`cargo fmt`; advisory budget was 60 —
kept: single runner + step table, mirrors CI `fmt|clippy|test|doc|all`).
`cargo run -p xtask -- fmt` exit 0. `lint` stays CI-only (documented in file).

## Explicit NOT GREEN / MANUAL (terminal — all owner-side, none loop-actionable)

`soak-72h` MANUAL · live `#[ignore]` MANUAL (standing no-tokens rule) ·
S2 djs-side TEMPLATE (structurally manual: needs token + 100–50k guilds) ·
geiger CI-only (uninstallable here; construction-green) · dogfood session not
evaluated · API freeze/tags/crates.io publish not done (no git repo here for
tags; no registry credentials for publish).
