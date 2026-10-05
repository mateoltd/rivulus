# 11 - Node.js compat pivot (napi-rs binding over the Rust core)

Status: draft for critique (steps 3-4 pending). Scope: everything below is
binding once this plan lands. History (plans 01-10, TRACKER, DOGFOOD) stays
as record and is NOT rewritten.

## 0. Goal in one paragraph

Ship a Node.js-compatible binary built on the existing Rust core: a napi-rs
binding crate plus TypeScript types so JS callers use discord.js-level
capabilities (Client builder, login, fetch_page/stream_pages, health,
cache stats, ed25519 verify) with the hot state living in Rust. The headline
requirement for the whole experiment is GC PRESSURE REDUCTION: every layer
choice below states how it serves that goal, and M5 measures it.

## 1. Non-negotiable conventions (binding from here on)

1.1 No em dashes anywhere in new or touched content (code, comments, docs,
README, descriptions). Replacement rule: hyphen or colon, or reword. Verify
with (only over trees that exist; never over dependency or build output):

hits=$(for d in crates examples node bench/djs-equiv; do
  [ -d "$d" ] || continue
  grep -rn $'\u2014' "$d" README.md MIGRATION.md CHANGELOG.md \
    --exclude-dir=target --exclude-dir=node_modules || true
done); test -z "$hits"

Rationale: `node/` does not exist at M0 (a bare grep would error), and from
M1 on it contains `node_modules/` plus build output that third parties fill
with em dashes. plans/ and .tmp/ are historical record and scratch:
excluded on purpose. bench/djs-equiv/ measured 2 hits on 2026-10-05, so the
M0 purge covers it (no scope mismatch).

1.2 Purge slop comments; do not reword them. Delete comments that reply to
direct instructions (example: notes about naming prefixes), internal phase
refs (example: frozen-phase tags that mean nothing to a reader), and any
comment that adds no value to the reader. Deletion list for M0 (measured
2026-10-05): em dash hits in 10 files under crates/ plus the root examples
copy, webhook-verify-axum (2 copies), MIGRATION.md (39 hits), CHANGELOG.md
(1 hit), bench/djs-equiv/ (2 hits); phase tags such as the `frozen P1a`
markers in cache trait, dispatcher, client, members, messages docs. Keep
comments that document behavior, limits, error shapes, and safety reasoning.
M0 is mechanical deletions reviewed file by file in its turn; anything
ambiguous stays, with the ambiguity recorded instead of deleted.

1.3 No self-praise in user-facing text. Never write claims of the
"verified/working" family (the word "verified" may only appear next to the
command and output that produced the evidence). Published code is expected
to work; say what it does, not how good it is.

1.4 Mock-first for all CI tests that touch Discord (scripted gateway/REST;
the live harness in M6 is local-only connectivity, never CI, and this
sentence is the reconciliation). Live harness only via env token held as
SecretString, never printed or committed. State quiet-guild limits honestly
(current fixture: 1 guild, 4 channels, 0 messages; nightly djs compare stays
blocked below 100 guilds). No live writes without owner-visible naming
(`dogfood-smoke-*` prefix) and explicit owner approval in the turn brief.

1.5 One milestone per turn with evidence. Rust workspace gates (every
turn): `cargo fmt --check`, `cargo clippy --workspace --all-features
-D warnings`, `cargo test --workspace`, `cargo doc`
(`RUSTDOCFLAGS='-D warnings'`), `cargo deny check`. Node gates (from M1,
where they apply): `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test` scoped to `node/`, `npm ci`, `npm run build`,
`tsc --noEmit` over the checked-in `.d.ts` plus fixtures, `npm test`,
`npm run smoke` (must exit 0 without a token). The `node/` crate resolves
`../crates/rivulus` with default features only (the same configuration CI
tests); anything else needs a plan note first. Push commits only when
green. grep-verify no token before every commit. Each milestone states its
abort condition in its turn brief.

## 2. GC reduction: the per-layer story (priority requirement)

2.1 Rust core (unchanged semantics): hot state lives in Rust memory, never
in the V8 heap. Cache (`InMemoryCache`), gateway buffers, ratelimiter
buckets, standby routers stay behind the boundary. Kept pressure wins that
matter at the boundary: `Box<str>` cold fields keep snapshots small,
dashmap plus foldhash maps keep cache reads off the JS thread, bounded
queues with drop policies are the model for the bridge queue below.
(`AtomicU64` request ids are Rust-side hygiene, not a GC claim.) One honest
exception, stated now: the token is a JS string at `createClient` entry, so
it exists in the V8 heap from that call onward. The API never reads it back
(non-readable by construction), and mock secrets (never the live token)
are the only values M5 heap snapshots may contain.

2.2 napi boundary (new): crossings allocate in V8 by nature, so the design
keeps them narrow and one-directional:
- Handles are monotonic `u64` ids crossed as JS numbers, never recycled
  (ABA impossible by construction; exhaustion past 2^53 is documented as
  impossible-by-rate and untested). No napi `External` or class finalizers:
  finalizers run on GC timing, which makes close nondeterministic, while an
  explicit `close` is deterministic. Unknown ids reject with a typed
  `unknown-handle` error. A test-only live-handle counter (exposed only to
  tests, never in `.d.ts`) is asserted zero at process exit; that is the
  leak-test mechanism.
- JS holds snapshots, not state: snowflake IDs cross as strings (u64 does
  not fit in a double; any numeric snowflake is a bug), stats as numbers,
  pages as plain coarse arrays. Never hand a live Rust cache reference to
  JS; every read is a snapshot copy the caller can drop.
- Events cross only for subscribed kinds (see `subscribe` in section 3),
  as serialized JSON batches through one threadsafe function per client.
  One batch holds up to 64 events or 8ms of arrivals, whichever comes
  first. The bridge queue between Rust dispatch and the TSFN call is
  bounded and drops oldest on overflow, surfacing a counter in `getHealth`
  as `eventsDropped`. ArrayBuffer/columnar crossing was considered and
  deferred: JSON batches are debuggable and match core snapshot shapes, and
  M5 decides with numbers whether the parse spike matters; if the binding
  loses on allocation rate, columnar is the first fallback, not more
  batching.
- TSFN lifecycle is explicit because a referenced TSFN hangs process exit:
  unref while zero subscriptions are active, re-ref on subscribe, release
  permanently on close or on closed-failure. `close` is async: it drops the
  bridge queue, releases the TSFN, then resolves; post-close callbacks are
  impossible. In-flight `login`/`fetchPage` promises reject with the close
  error; double `close` succeeds trivially. A test asserting clean process
  exit after close is part of M4.
- Async model: the binding crate owns one dedicated single-thread tokio
  runtime; all `async fn` napi entry points run there via that runtime
  (never `block_on` from a libuv thread). Clients are shared by `Arc`;
  concurrent JS calls serialize inside core primitives as they do today.
- Errors reject as `Error` instances whose `.message` is the exact
  documented string (the contract file in M2 pins every string; matching
  the Rust `Display` text is the parity rule). No `.code` field in M3.
- TS types are declaration files: compile-time only. Their CI value is
  catching public API breaks at the npm boundary, nothing about runtime.
  Generated `.d.ts` is checked in and diffed against the M2 contract file
  (names, arity, sync-vs-async, return shapes), using the pinned generator
  and the documented regen command from M1. The diff catches shape drift;
  behavioral contracts (clamp rules, error strings) are asserted by tests,
  not by the diff.

2.3 JS consumer shape (new, thin): the example bot reads snapshots and
drops them (no per-message caches, no retained closures over pages). The
discord.js baseline arm in M5 uses an equally thin consumer (caches
disabled); otherwise the comparison measures consumer discipline, not the
boundary.

2.4 Honest limits and the single gate (stated up front, not discovered
later): crossing the boundary still allocates; the win is where state lives
and how rarely JS must cross. There is exactly one verdict rule, and it
replaces both earlier formulations: the M5 turn pre-registers numeric bands
first (bytes allocated per message from `--trace-gc` deltas over an
iteration loop, survivors after forced GC as secondary, RSS only as a
Rust-side leak tripwire), then runs, then compares medians with dispersion
over repeated runs. If the binding allocates more per message than the
baseline on the identical mock workload, the milestone fails and M5-fail is
a legitimate stop-or-pivot outcome (columnar crossing first, narrower
crossings second), not a refactor mandate. No warn-only landing: bands are
pre-registered, runs are counted, the verdict is written down.

## 3. API boundary (napi surface, discord.js parity naming)

JS-facing modules (names mirror MIGRATION.md so discord.js users transfer):

- `createClient({ token, intents, cache, sharding })` returns an opaque
  numeric handle. Shapes: `intents` is a bitfield number, `cache` is one of
  `"minimal" | "balanced" | "full"`, `sharding` accepts `"auto"` only (any
  other value rejects with a typed error; multi-shard behavior is
  untested-for-now, see section 5). Token stays in Rust as SecretString and
  is never readable back through the API; it is necessarily present in the
  V8 heap at entry (see 2.1).
- `subscribe(handle, kinds: string[])` / `unsubscribe(handle, kinds)`:
  controls which event kinds cross the TSFN (created on first subscribe,
  unref rules per 2.2). No subscription means no event delivery at all.
- `login(handle): Promise<void>` resolves after 30s READY discipline or
  rejects with an `Error` whose `.message` is the exact documented string
  (`timeout: READY timeout`, `unauthorized: check token`, `network: ...`).
- `fetchPage(handle, channelId, limit): Promise<Message[]>` and
  `streamPages(handle, channelId): AsyncIterable<Page>` where each `Page`
  is one coarse pre-shaped array crossing. Same clamp rules as Rust (`0`
  maps to `1`), same live-only errors. The binding holds the paging cursor
  (last-seen id passed as `after`, mirroring core) so JS iteration needs no
  cursor math; an unconsumed iterator dropped by explicit `return()` frees
  its Rust allocation (tested). No per-item crossing, no retained graphs.
- `getHealth(handle)`, `getLatencies(handle)`, `getUptimeMs(handle)`,
  `getCacheStats(handle)` are synchronous. `getHealth` includes
  `eventsDropped`; `getLatencies` returns `{ shard, total, latencyMs }[]`.
  Message snapshots carry IDs as strings, always.
- `verifyWebhook(publicKeyHex, timestamp, body: Buffer|Uint8Array,
  sigHex)` is sync and pure (no handle, no I/O); same concat rule
  (`timestamp_bytes || body_bytes`) and same fail-closed errors as
  `interactions::verify`, plus a typed hex-decode-failure error. No string
  overload: UTF-16 re-encoding cannot be byte-exact.
- `close(handle): Promise<void>` drains-or-drops the bridge queue, releases
  the TSFN, then resolves; post-close callbacks are impossible. In-flight
  `login`/`fetchPage` promises reject with the close error; double `close`
  succeeds trivially; unknown ids reject with `unknown-handle`.
- Snapshots shape (pinned by the M2 contract): `{ id: string,
  channelId: string, authorId: string, content: string,
  timestamp: string }` plus explicitly listed extras only.

Out of scope for the binding (stated, not silent): modal/autocomplete
constructors, slash PUT helpers, chunk requests, voice (stub in core),
lag-policy tuning, guard combinators. These stay owner-decision items from
the dogfood triage; the binding exposes no wrappers for APIs that do not
exist in core.

## 4. Crate layout and toolchain facts (checked 2026-10-05)

- New top-level `node/` directory, NOT a `crates/*` workspace member. It
  has its own `Cargo.toml` (path dependency on `../crates/rivulus`),
  `package.json` (napi build scripts, engines `>=20`), and its own
  `Cargo.lock`. Reason: the 1.75 MSRV CI job builds `--workspace`; napi-rs
  tracks recent Rust and must never break that job. The `node/` crate builds
  on stable only, in its own CI job.
- `deny.toml [graph] all-features` covers the workspace lock only; `node/`
  gets its own deny pass starting in M1 (same command scoped to `node/`,
  same fail rules; the shipped binary has the strictest supply-chain gate,
  not the weakest).
- Local baseline: Node v24.21.0, npm 11.19.0, discord.js ^14 skeleton
  already in `bench/djs-equiv` (engines `>=20`). CI matrix for node: 20,
  22, 24 (floor, maintenance LTS, local truth). No prebuilds: the package
  builds from source, which documents the experiment as workstation-scoped
  until a packaging milestone says otherwise.
- napi version choice is an M1 verification step, not an assumption: pick
  the newest napi-rs 2.x whose manifest resolves on stable here AND whose
  docs state MSRV at or below the stable toolchain; pin exact `=x.y.z` for
  `napi` and `@napi-rs/cli`, record version plus MSRV plus the regen
  command in M1 evidence, and commit `node/Cargo.lock`. If no 2.x
  qualifies, stop and report (do not jump to v3 silently).

## 5. Test methodology (mock-first, live harness rules)

5.1 Mock-first boundary suite (M4): a scripted gateway (Hello, Identify,
READY with 1 guild / 4 channels / 0 messages, then MessageCreate fixtures)
plus a scripted REST mock drive the napi functions under Node's test
runner. Injection seam (stated now, verified in M3): a test-only
constructor path that points the binding at mock transports; if core
cannot take injected transports, the mocks are loopback WebSocket/HTTP
servers on 127.0.0.1 and the suite asserts no non-loopback socket opens.
Either way M3 names the seam before M4 is built; if the seam needs core
refactoring, M3 sizes it with an abort option instead of absorbing it
silently. Assertions: snapshot shapes against the M2 contract, error
strings on happy and failure paths (redaction asserted on failures too,
including base64 transforms of the mock secret), the `0 maps to 1` clamp,
login 30s discipline, post-close rejection, mid-stream `streamPages`
failure, disconnect visibility (event or documented silence), disposal
(close-then-call rejects, close during pending calls rejects them, double
close succeeds, clean process exit after closing all handles), live-handle
counter zero at exit, no token anywhere in logs. Explicitly uncovered (not
silent): multi-shard behavior (single-shard fixture only).

5.2 GC measurement (M5): the M5 turn publishes fixed parameters first
(guild/message counts, message size mix, seeding, warmup, repeat count of
at least 3, one pinned Node version for the numbers, pre-registered
tolerance bands with median-plus-dispersion verdict), then runs, then
compares. Arms: (a) napi binding + thin consumer from M3, (b) discord.js
equiv with caches disabled against the same mocks, (c) binding with a
retaining consumer as a canary for accidental rewrites (not evidence),
(d) discord.js with default caches (documents how much of any win is
consumer configuration). Primary metric: allocated bytes per message from
`--trace-gc` deltas over an iteration loop; survivors after forced GC
secondary; RSS only as a Rust-side leak tripwire. The mocks serving (b)
are loopback socket servers faithful enough for discord.js gateway/REST
handshake plus 429 handling; the shared fixture format (frames both arms
must replay) is defined in the M5 turn, seeded by live-captured redacted
frames so mocks replay Discord reality, not the author's model. Verdict
leans on (a) vs (b) plus (d); per section 2.4 a fail is a legitimate
stop-or-pivot outcome.

5.3 Live harness (M6, env token only, local-only, never CI): a read-only
`login` + `fetchPage` connectivity check on the quiet guild plus
`getHealth`/`getLatencies`, logged redacted as guild-total aggregates and
error strings (never per-channel counts, never IDs, never the token). This
is plumbing evidence, not surface validation: the fixture holds 0 messages
so pagination and snapshot shapes get no live coverage. The redacted log is
committed as a dated evidence file under `bench/results/` so later turns
can diff it. No writes. If a write is ever needed it gets its own owner
turn with `dogfood-smoke-*` naming first.

## 6. Milestones (one per turn, each with section 1.5 evidence)

- M0 hygiene sweep: purge list from 1.2 (em dashes + slop comments),
  with one guard: doc code blocks (```rust examples in /// and //!
  comments) are behavior, not slop. Keep every doctest compiling; the
  `cargo test` gate enforces this. Grep zero, gates green, commit, push.
- M1 `node/` scaffold plus minimal node CI: napi crate builds on stable,
  `npm run smoke` exits 0 token-free (loads binding, prints version string
  only), napi version + MSRV + pins + regen command recorded,
  `node/Cargo.lock` and `package-lock.json` committed, node-scoped Rust
  gates and scoped deny pass from this milestone on, CI job created with
  the 20/22/24 matrix (npm build + smoke + scoped gates only for now).
- M2 contract file: hand-written surface contract (names, arity,
  sync-vs-async, return shapes, error strings) reviewed as docs; no
  generated output yet.
- M3 method surface: all section-3 functions live against mocks with the
  GC rules from 2.2 enforced, generated `.d.ts` diffed against the M2
  contract in CI, `tsc --noEmit` fixture green, plus a thin example
  consumer under `node/` implementing section 2.3 (read snapshots, drop
  them, retain nothing). This consumer is also the M5 workload (a); it is
  written once here, measured there. M3 also names the mock injection
  seam from 5.1 (or sizes the refactor with an abort option).
- M4 mock-first boundary suite in CI (Node job, no token), per 5.1.
- M5 GC measurement with pre-registered bands and verdict, per 5.2.
- M6 live harness connectivity run with committed redacted evidence.
- M7 docs (README section, MIGRATION row, hygiene re-check), `npm
  pack` dry run (source build; prebuilds explicitly out of scope).

## 7. Open risks (not decisions)

- napi-rs version MSRV vs stable-only `node/` job (resolved in M1).
- Mock-seam residual: if M3 finds core cannot take injected transports,
  the loopback-server fallback is sized there, not here.
- djs socket-server fidelity bounds the M5 number; shared redacted
  fixtures bound the drift.
- No prebuilds (workstation experiment, stated in section 4).

## 8. Decision log (append-only)

2026-10-05 critique round (verbatim files under `plans/11-critiques/` from
opencode-go/deepseek-v4.1-flash, opencode-go/glm-5.3-flash,
opencode-go/space-bunny-free). All three agreed on: handle-model
contradiction, M2-before-M3 ordering, `streamPages` incoherence,
`verifyWebhook` bytes, string-only errors, TSFN release, runtime
ownership, GC metric misalignment, djs baseline plumbing, mock seam.
Resolutions, with dissent handled explicitly:

- Handles stay numeric monotonic u64, never recycled, plus unknown-handle
  error and test-only live-handle counter (all three demanded a mechanism;
  deepseek/bunny leaned napi class, glm defended numeric with `Disposable`
  support). Decided numeric because finalizers run on GC timing, which
  makes close nondeterministic, while explicit close is deterministic and
  cheaper to assert. `Symbol.dispose` nicety deferred, not required.
- M2 is now the hand-written contract file; typegen output diffs against
  it from M3 on (all three called the order impossible as written).
- `streamPages` is `AsyncIterable<Page>` of coarse batches with a
  binding-held cursor (`after`, mirroring core), explicit `return()`
  frees; per-item crossing rejected (all three).
- `verifyWebhook` takes `Buffer|Uint8Array` only, plus hex-decode error
  (all three; no string overload).
- Errors reject as `Error` with `.message` pinned by contract, no `.code`
  in M3 (all three demanded Error semantics; code field deferred as
  unneeded surface).
- Primary metric is allocation rate via `--trace-gc` deltas,
  pre-registered bands, verdict on (a) vs (b)+(d), (c) demoted to canary,
  M5-fail is legitimate stop-or-pivot (all three showed heap-after-GC
  alone cannot falsify the claim; deepseek/bunny wanted rate, glm wanted
  steady-state protocol - all adopted).
- TSFN unref/ref/release-on-close, close drains-or-drops, in-flight
  rejection, double-close ok, clean-exit test (all three).
- Binding owns one dedicated single-thread tokio runtime (deepseek 3b,
  glm 1b: unspecified was the largest risk; decided, not deferred).
- Token-in-V8-heap stated honestly; mocks use mock secrets only
  (deepseek 3c, bunny point 10).
- napi CLI pinned with regen command, `node/Cargo.lock` committed, matrix
  20/22/24, prebuilds explicitly out of scope as workstation experiment
  (bunny 25/26, glm 1j/3e/3f; Node 22 added per both).
- Node CI job created in M1 minimal and extended later; node-scoped Rust
  gates plus `tsc` plus scoped deny from M1; M7 slimmed to docs plus pack
  (deepseek 1g/1h, bunny 12/29, glm list item 10; M7-overload fixed).
- M0 kept in position but scoped mechanical plus file-by-file review
  (bunny argued drop/split; kept because gates catch regressions and the
  diff is small, with ambiguous comments recorded not deleted).
- ArrayBuffer crossing considered and deferred with revisit condition
  (glm 2a; JSON batches first for debuggability).
- djs cache-off arm (d) added; consumer discipline equalized (bunny 16,
  deepseek 2d).
- Live-captured redacted frames seed the shared fixtures (addresses
  bunny 35 mock-fidelity concern within honesty limits).
- M6 is connectivity evidence with committed artifact, aggregate-only
  logs, local-only (deepseek 4d, bunny 36/39, glm 2f/4d).
- Sharding accepted-and-validated, multi-shard explicitly uncovered
  (glm 4c, deepseek 1e).
- Where critics disagreed with each other: none did on substance; glm
  alone praised 2.4's failure condition (kept and strengthened) and noted
  the idle-scoping style detail (adopted as pre-registration discipline).

2026-10-05 M3 execution notes (surface as built, all green):
- One TSFN per subscription, not per client: re-subscribing aborts the
  previous one, so at most one is ever live per client. Unsubscribe and
  close abort plus drop; zero subscriptions means zero live TSFNs, which
  satisfies the unref rule by construction.
- `streamPages` returns eager nested arrays (napi 2.16 has no generator
  support); the `Page` alias stays contract-prose only so `tsc` passes.
- napi `dyn-symbols` enabled so the `cargo test` harness links without a
  Node process present; the shipped `.node` resolves symbols from Node.
- Mock seam is additive only: `ClientBuilder::base_url` (mirrors the
  existing REST builder hook) plus the pre-existing `login_with_url`;
  no core behavior changed.
- u64 never crosses the boundary (napi 2.16 has no plain-u64 JS mapping):
  latencies and uptime cross as `f64`, IDs as strings, per contract.
- Dispatch routing uses the `Unknown` inner kind (core forwards non-core
  dispatches as `Event::Unknown`, whose `kind()` bucket is always
  `"UNKNOWN"`); subscribers match Discord kinds. Listeners are error-first
  `(error, batch)` per the napi `CalleeHandled` strategy, pinned in the
  contract after a live failure proved the first-arg shape.
