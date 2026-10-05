# Critique: plans/11-node-compat-pivot.md

## 1. Correctness gaps in the API boundary and milestone sequence

**1a. Error model contradicts itself.** §3 specifies "rejects with the typed error string" and then enumerates colon-prefixed string families (`timeout: READY timeout`, `unauthorized: check token`, `network: ...`). A string is not a typed error, and there is no exhaustive grammar or stable discriminant. Callers must prefix-match, which breaks as soon as a network message contains a colon or the message text changes. If Discord parity is the goal, this should be a discriminated error (code enum + message), not a magic prefix. As written it cannot be listed as "matching `Client::login` semantics exactly" (§3): string families are a lossy projection of a Rust error enum.

**1b. Handle model is internally inconsistent and unsafe.** §3 returns "an opaque handle (numeric id)", but §2.2 asserts "No `External` values escape without an explicit `close`/`dispose`." A plain numeric id is not tracked by V8 at all, so `close`/leak enforcement must live in a Rust-side registry. The plan never specifies that registry, its capacity, or the ABA hazard: if ids are reused after `close`, a stale JS handle silently drives a different client. "Leaked handles fail M5 tests" is unachievable with a bare number and no finalizer, while adding a `FinalizationRegistry`/`External` reintroduces the runtime cost §2.2 claims to avoid. These two claims cannot both hold.

**1c. `streamPages` is incoherent.** §3 says it returns "plain arrays/iterables of message snapshots." A plain array materializes everything (defeating §2.4's "narrower crossings, coarser snapshots") while an iterable implies one boundary crossing per `next()`. Worse, the API has no cursor: `fetchPage(handle, channelId, limit)` returns no `before`/`after` token, so `streamPages` has no way to advance and cannot page past the first batch. That is a hard parity gap versus discord.js `fetchMessages({before,after,around})` and makes the function unimplementable as described.

**1d. `verifyWebhook` body type is a security-boundary bug.** §3 passes `body` as a JS value with no type. A JS string is UTF-16; Discord verification must run over the exact raw bytes (`timestamp_bytes || body_bytes`, §3). Re-encoding an invalid or non-ASCII body from UTF-16 can change the bytes, giving either spurious failure or signature confusion if the re-encoded bytes are what get verified. The binding must accept a `Buffer`/`Uint8Array` and must define hex-decode failure behavior; §3 does not.

**1e. `sharding` option is a silent no-op risk.** `createClient({ token, intents, cache, sharding })` (§3) accepts `sharding`, but sharding is not described as implemented and the core is only "shard down" error family. Accepting an option that core may not honor is exactly the silent gap §3 and §1.3 pledge to avoid; it should be rejected or documented as accepted-and-ignored.

**1f. Milestone M2/M3 order is backwards.** §2.2 requires checked-in generated `.d.ts` diffed in CI, and M2 is "TS types" *before* M3 "all section-3 functions live." napi typegen emits signatures from existing `#[napi]` items, so at M2 there is nothing to generate from. M2 can only pass with hand-written stubs, which then drift the moment M3 lands, or M2 is vacuous. Surface (M3) must precede generated types, or M2 must be explicit hand-authored placeholder types with a stated reconciliation step.

**1g. CI ordering is self-contradictory.** M5 says the mock boundary suite runs "in CI (Node job)", but the node CI job is created in M7. Likewise M4's gate is "after one clean nightly" (§5.2) while no nightly/bench harness exists until M7. So M4-M5 evidence is not actually enforced when those milestones close.

**1h. Per-milestone Rust gates don't cover the work.** §1.5 requires `cargo clippy --workspace --all-features`, `cargo test --workspace`, `cargo doc`, but §4 says `node/` is *not* a workspace member and has its own lockfile. So none of those gates build or test the napi crate; there is no stated `cargo fmt/clippy/test/doc` pass *inside* `node/`. M1 evidence reduces to "`npm run smoke` prints a version," which exercises zero boundary calls, so M1-M2 can be green while the boundary is entirely broken.

## 2. Does the GC-reduction story hold together?

**2a. The success criterion is stated two different ways.** §2.4 fails the milestone if "the binding allocating more per event than the discord.js baseline," but §5.2 gates on "heap-after-GC with tolerance bands." Per-event allocation and heap-after-GC are different quantities: a binding can allocate more per event and still show lower post-GC heap (GC collected it), or vice versa (lower churn but retained external memory). The plan calls both "the" gate and never reconciles them.

**2b. The headline metric cannot falsify the headline claim.** "GC pressure" is allocation rate and promotion, but §5.2 measures V8 heap after forced GC, heap-snapshot object counts, and RSS. Heap-after-GC only sees the JS side, which is thin *by construction* (§2.3), so it will "win" trivially. Rust memory is invisible to V8 heap; only RSS sees it, and RSS is noisy and rank-deferred in the gate. The actual risk, that the work merely *displaced* memory into Rust rather than reducing GC pressure, is not measurable by the chosen primary metric. Missing metrics that would: minor/major GC counts (`--trace-gc`), allocation sampling, `v8.getHeapStatistics().total_heap_size`, and external-memory accounting (napi `Buffer`/`External` count against V8 external memory and can themselves trigger GC, which §2.2 never mentions).

**2c. The event path likely *increases* JS churn.** §2.2 crosses events as "serialized JSON strings through a threadsafe function, batched per tick." Every batch allocates a JS string plus a `JSON.parse` object graph, which is precisely the short-lived promotion pressure the milestone claims to reduce. Nothing measures allocation rate, so the design could win heap-after-GC while losing on GC frequency. The batching size and "tick" definition are also undefined.

**2d. The measurement confounds two variables.** Workloads are (a) binding + thin consumer, (b) discord.js equiv, (c) binding + retaining consumer. (c) is a retention control, not a runtime control; it validates the harness, not the binding. The runtime comparison (a) vs (b) is only valid if (b) uses an equally thin consumer, and §5.2 does not say that it does. As written, (a) vs (b) may measure consumer discipline, not the Rust boundary. Also, §5.2(c) states the negative control "must measure worse" as fact; it should be a pre-registered expectation with a defined outcome if it does not (harness invalid).

**2e. "TS types are compile-time only: zero runtime GC cost" (§2.2)** is true of `.d.ts` but not of napi's generated JS glue, which can allocate wrappers per call; the claim should be scoped to the declaration files.

## 3. Node boundary risks (napi specifics)

**3a. Threadsafe functions and event-loop exit are unhandled.** §2.2 uses a TSFN for events; napi requires `ThreadsafeFunction::release()`/`unref`. A TSFN that is not released on `close` keeps the libuv loop alive, so a bot process hangs on exit. §3 lists no `unref`/release path and no test asserts clean process exit. Backpressure is relegated to "open risk" (§7) with "coarser batching" as a later decision, so there is no bounded queue and no drop policy at the boundary at all.

**3b. Async execution model is unspecified.** `login`/`fetchPage` are async; napi-rs runs `async fn` on the libuv threadpool, requiring `Send` futures, and the core client is likely tied to a tokio runtime. Whether the binding owns a runtime, reuses the core's, or does `block_on` from a libuv thread (deadlock risk) is never said. Sharing a client across concurrent JS calls (borrow vs `Arc`, `&mut` serialization) is also undefined. This is the single largest boundary risk and it is absent.

**3c. Token secrecy is overstated.** §3/§1.4 say the token "stays in Rust as `SecretString`; it is never readable back." But `createClient({token})` receives a JS string and napi copies it, so the token is present in the V8 heap and a heap snapshot regardless. The API is non-readable, but the stated security posture is not the same claim. Redaction is only mandated for the live harness (§5.3), not for core tracing/error strings that can surface at runtime.

**3d. Version pinning is incomplete.** §4 pins the Rust MSRV decision to M1 but does not pin `@napi-rs/cli` (npm), the Node-API level (`engines >=20` is not a Node-API version pin), or prebuilt ABI targets. For a source-built binding this may be tolerable, but M7's `npm pack` dry run tests packaging without any prebuild/arch/glibc strategy; consumers on other platforms get no artifact, and that gap is not listed as a risk.

**3e. Typegen drift enforcement is underspecified and noisy.** "Generated `.d.ts` checked in and diffed in CI" (§2.2) depends on a CI job not created until M7 (see 1g), and napi typegen output is not guaranteed byte-stable across generator versions. Without pinning the generator and a normalization step, the "loud failure" becomes recurring noise and will be bypassed. The diff also catches only shape drift, not the behavioral contracts (clamp rules, error strings) that matter most here.

**3f. Numeric handle width.** §3's numeric handle has no declared integer width; if crossed as `f64` and the curve ever exceeds 2^53 it silently loses identity. Related: §2.2 promises snowflakes cross as strings, but no snapshot shape is specified, so a stray numeric snowflake in `fetchPage` could silently round.

## 4. Test-methodology holes

**4a. "In-process" mock claim is unverified.** §5.1 says a scripted gateway/REST "drive the napi functions in-process." If the binding drives the real Rust gateway handling, the mock must be a real loopback WebSocket/HTTP server, not in-process, and the tests are integration tests. §5.1 should state the injection seam (trait mock vs loopback server) and assert no non-loopback socket is opened.

**4b. The token grep test is necessary but not sufficient.** Grepping captured logs for the exact mock secret proves only absence in that output; it does not catch base64/partial/transformed leaks, and if the layer never logs, the assertion is vacuous. It also says nothing about in-memory presence (see 3c).

**4c. Measurement validity is undefined.** §5.2 names "fixed mock workload (N guilds, M messages)" without N, M, seeding, or payload determinism, which is required for a fair (a)/(b) compare. There is no stated repetition count, warmup/JIT-tiering control, single pinned Node version for benchmarks (the matrix is 20 and 24), or definition of the "tolerance bands" (absolute vs relative, derived from what variance). (b) discord.js "against the same mocks" is nontrivial: discord.js does not trivially accept mock gateway/REST endpoints, so the baseline may end up on a different workload and invalidate the comparison; no mechanism is given.

**4d. Live harness is thin and its safety rules are unenforced.** §5.3 runs read-only `login` + `fetchPage` on the quiet guild, which §1.4 fixes at 0 messages, so pagination/GC paths get no live coverage and the only real signal is health/latency. Redaction is asserted but no redactor is specified and no test verifies the redactor, even though Discord errors routinely embed channel/guild IDs. "No writes" is a rule, not a mechanism; there is no read-only token scope enforcement or method allowlist, and no assertion that a write call is rejected in live mode.

**4e. Disposal/lifecycle coverage is incomplete.** §5.1 asserts only "close-then-call rejects." Missing: close during pending `login`/`fetchPage`, double `close`, id reuse after `close` (see 1b), and the TSFN/event-loop exit case (3a), which only a test asserting clean process exit would catch.

**4f. Cross-check absent between boundary tests and Rust tests.** §5.1 checks snapshot shapes and error strings but not the boundary translation edge cases that are most likely to break: i64/string conversion for IDs, clamp boundary (`0 -> 1`, §3), number/string typing of stats, and the exact `verifyWebhook` byte concatenation. M2's fixture only asserts names exist, so shape/type drift slips through both gates.

## Summary of plain disagreements

- §2.2 and §3 conflict: no `External`/leak enforcement with numeric handles.
- §2.4 and §5.2 conflict: two different success criteria, never reconciled.
- The §5.2 metric set cannot falsify the §0 headline claim; it measures JS displacement, not GC pressure.
- §3 `streamPages` is unimplementable (no cursor) and self-contradictory (array vs iterable).
- §3 `verifyWebhook` should take bytes, not a JS string; current text is a signature-confusion hazard.
- M2 before M3 is backwards; M5 "in CI" and M4 "nightly gate" precede the M7 CI job.
- §1.5 Rust gates do not cover `node/` at all; M1 smoke is near-zero evidence.
- TSFN release/unref, the async runtime/`Send` model, and backpressure are missing, not merely "risks."
- Live harness is weakest exactly where it claims to be evidence, and its safety rules have no enforcement mechanism.

No network or secret access was used; only plans/11-node-compat-pivot.md was read.
