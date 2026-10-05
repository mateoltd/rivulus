# 07 — Testing, CI, Docs & Release

## 1. Test pyramid (offline default; `#[ignore]` for live/soak-72h)
- Unit per crate: `model` serde fixtures (`tests/fixtures/*.json` >=50, docs+djs captures), permissions calc, bucket-key fn, intents bits, `standby` predicates, `ed25519` vectors, `Unknown(u8)` round-trip, validation limits.
- Integration: `tests/support/mock_gateway.rs + mock_rest.rs` (`127.0.0.1:0` deterministic) used by `tests/gateway_resume.rs` (hello->identify->dispatch->drop->resume + close-code matrix), `tests/rest_ratelimit.rs` (bucket exhaust->429->retry_after; shared-scope excluded), `tests/cache_diff.rs` (old/new without full-guild clone), `tests/interactions_verify.rs`, `tests/collectors.rs` (filter+max+idle+timeout+end-reason).
- Short chaos: `tests/soak.rs --short` (10min seeded kills, resume-rate assert, 0 panics). `#[ignore] soak-72h` manual only.
- Live `tests/live_*.rs #[ignore]` (`DISCORD_TOKEN+GUILD_ID`): ping/slash/button/modal/autocomplete smoke (voice excluded v1).
- Coverage `tarpaulin >=80%` on `rest,gateway,cache`.

## 2. CI (`.github/workflows/ci.yml`)
Mocks: `tests/support/mock_{gateway,rest}.rs` expose `async fn spawn() -> SocketAddr` (ephemeral `127.0.0.1:0` via oneshot, scripted hello/identify/dispatch/drop + bucket-exhaust/429).
Jobs: `fmt(check) -> clippy(-D warnings, pedantic subset) -> build matrix (linux/win/mac x 1.75+stable, dtolnay toolchain; NO rust-toolchain.toml) -> test --workspace --all-features -> doc(-D warnings, --no-deps --all-features) -> deny + `cargo geiger --forbid-only --package rivulus-*` + audit -> bench-quick (tolerance bands) -> live-manual(dispatch only)`. MSRV pinned job. Cache `~/.cargo`. Lint step: `grep -rn unwrap|expect src/` fails (allow only `#[cfg(test)]`). `cargo publish --dry-run` per crate.

## 3. Docs & examples (dogfood gate)
- Every public item `///` + `# Example` (doctests run); module docs include `djs -> rivulus::` snippet with SHORT imports.
- `examples/`: `ping.rs`, `slash.rs`, `buttons-collector.rs`, `sharded.rs`, `cache-tuning.rs`, `webhook-verify-axum.rs` (dev-dep axum only), NO `voice-send.rs` v1 (voice stub-only D6). Each runnable `cargo run --example` vs mocks + `#[ignore] live` notes.
- Root `README.md`: parity badge, RAM graph (measured), 5-min quickstart (short imports), djs migration table, feature flags table, voice-deferred notice.
- `CHANGELOG.md` + `MIGRATION.md` + `plans/DECISIONS.md` (P3 WS choice, hasher, etc.).

## 4. Release train
SemVer; tags `rivulus-vX.Y.Z` per package (facade version = release; inner crates independent). Pre-1.0 `0.x`; 1.0 when S1-S5 (minus voice) green. `cargo release`-style; weekly `cargo audit`.

