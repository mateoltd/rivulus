# Critique: plans/11-node-compat-pivot.md

## 1. API boundary and milestone-sequence correctness gaps

**1a. The event model contradicts itself: there is no subscription API (§2.2 vs §3).** §2.2 promises events cross "only for subscribed kinds," but §3's entire surface is `createClient`, `login`, `fetchPage`/`streamPages`, getters, `verifyWebhook`, `close`. Nothing subscribes to event kinds. Intents (§3) filter what Rust *receives*, not which kinds JS *wants delivered*. Add an `events` option to `createClient` or an `on(handle, kind)` method, or delete the "subscribed kinds" claim. This is a binding-mechanism gap, not a nicety: it also determines whether a TSFN is needed at all before login.

**1b. Executor/runtime ownership is unspecified and load-bearing.** The core is async (login waits on READY, the gateway task runs continuously, streamPages streams). The plan never says who runs it: a dedicated Tokio runtime owned by the crate, which thread pool size, whether `close` shuts it down or parks it, and how async fns are plumbed (e.g. `execute_tokio_future`). If the intent is to lean on napi-rs's built-in tokio runtime feature, say so and note that it is a single dedicated runtime thread, which bounds gateway/ratelimiter/standby concurrency throughput. Every hang/zombie-thread hazard below traces to this omission.

**1c. `close()` has no delivery-drain semantics.** Events arrive "batched per tick" through a TSFN (§2.2). After `close(handle)` (§3), what happens to in-flight batches and the unbounded TSFN queue? Callbacks firing into a closed client is the canonical napi crash. Specify: close drains or drops the queue, then unrefs, and post-close callbacks are impossible.

**1d. `streamPages` returning "plain arrays/iterables" (§3) is unresolved, and iterables contradict the design.** A Rust-backed iterator crosses the boundary per item, which is precisely the "crossings must be rare" rule §2.4 declares. Arrays cross once with a large burst. These are opposite trade-offs for a GC-driven design; the plan must choose and defend.

**1e. "Rejects with the typed error string" (§3, several functions) conflicts with generated behavior.** napi-rs `Result<T>` surfaces as thrown `Error` instances whose message is the string. The prose contract and the typegen output will diverge; §2.2's CI type-diff will then flag drift nobody designed. Decide now: Error instances with typed code/message fields, and rewrite §3 prose to match before M3 implements it.

**1f. Leak testing is unobservable.** §2.2: "Leaked handles fail M5 tests." Handles are numeric registry ids (§3); JS never knows GC finalized them, and numeric handles cannot run finalizers. A leak test needs a test-only registry-size probe (count exposed to the test surface). Neither exists. As written, M5 cannot assert the claim.

**1g. M2 is not executable in the given order.** M2 checks in "generated `.d.ts`" with a fixture asserting the documented surface exists. Typegen emits from `#[napi]` Rust signatures, which only exist at M3. Without explicit signature stubs at M2, M2 tests nothing. State the stub step or fold typegen into M3.

**1h. The node crate escapes every §1.5 gate.** §4 deliberately excludes `node/` from the workspace, but §1.5's gates (fmt, clippy, doc, test, deny) are workspace-scoped and §1.5 adds only `npm run smoke`. So `node/`'s Rust code fails no lint/format/test gate for the whole experiment. Specify the node CI job content (M7) and Rogers M1: `cargo fmt --check`, `clippy -D warnings`, `cargo test` for the node crate, plus `npm ci && npm run build && npm run smoke`.

**1i. §1.1's verify command has two scope bugs.** (1) It greps `node/` at M0/M1, which does not exist at M0 (grep errors, ambiguous exit code) and contains `node_modules/` plus build `target/` from M1 onward; third-party JS and binaries will contain em dashes, so the "must print nothing" gate fails forever. The exclusion list in 1.1 must name `node_modules`, `node/target`, and sandbox dirs. (2) It greps `bench/djs-equiv/`, but the M0 measured deletion list (1.2) never includes bench, a mismatch between verify scope and purge scope. If bench is dirty, M0's list is stale; if clean at measurement, the list should say so.

**1j.里面的锁文件固定双锁文件与 crate 版本未固定。** §7 says npm package-lock.json is committed from M1, but never says node/ `Cargo.lock` is committed. Commit it; an uncommitted second lock makes the deny pass (M7) and reproducibility meaningless. Also: M1 records "version plus MSRV" but no pinning instruction (e.g. exact `=x.y.z` for `napi` and `@napi-rs/cli`); without a pin, MSRV qualifications made in M1 decay silently on a transitive bump.

## 2. Does the GC-reduction story hold? Mostly the frame does; several load-bearing holes

**2a. "JSON string batch per tick vs JS parse spikes" is a self-inflicted allocation concentration.** Batching per tick then `JSON.parse` over M message graphs in one tick creates a worse V8 allocation spike than per-event allocation spread across ticks. §5.2 never measures this (single heap-after-GC point can't). The plan asserts the win but not the mechanism versus its own baseline. Consider payload-as-ArrayBuffer crossing (typed views, no JSON.parse at all), which is the near-zero-V8-copy option for a plan whose sole purpose is GC pressure. A GC-goal plan that never mentions ArrayBuffers is a design gap; at minimum record why the option lost.

**2b. §2.4's failure condition is unmeasurable by §5.2's metrics; and the two sections contradict each other on the verdict.** §2.4: "if the binding is allocating more per event than the discord.js baseline ... milestone fails" - a hard fail. §5.2: "warn-only first landing, gate after one clean nightly" - a soft gate. Pick one. Worse: "allocates more per event" cannot be computed from heap-after-forced-GC, snapshot object counts, and RSS - those measure retention, not allocation rate. Add allocation sampling (`--heap-prof`) or forced-GC round deltas over an iteration loop, or rewrite the gate in retention terms.

**2c. §5.2 measurement lacks a steady-state protocol.** One forced GC and one snapshot is noise. Require: warmup pass; K forced GCs until consecutive heap deltas plateau; median over ≥3 repeated runs; same Node binary/date for arms (a), (b), (c); identical tick-rate and message shape mix to both drivers. None of this is stated. "Tolerance bands same style as bench/compare.py" cites a script whose contract is not sketched in the plan.

**2d. The negative control (c) nearly validates nothing.** A consumer retaining everything is definitionally worse under forced GC. Keep it as a canary for accidental consumer rewrites, but don't count it as evidence the binding wins; §6 M4 verdict must lean on (a) vs (b).

**2e. djs baseline plumbing is assumed, not built.** `bench/djs-equiv` is described only as a "^14 skeleton" (§4). Driving discord.js against a scripted gateway requires injecting a fake ws/REST layer into a live Client - a real chunk of work being treated as prefabricated. Same mocks must yield the same frames to each arm; no shared fixture format is defined (N, M, event mix, message distribution are all "TBD until M4").

**2f. Live workload's power is mis-described.** 1.4's quiet-guild fixture is 1 guild / 4 channels / 0 messages. So M6's live `fetchPage` exercises only the empty path; treat it honestly as a plumbing smoke, not a capability datapoint (current wording elsewhere treats live runs as meaningful reads). And per-channel counts in M6 logs would identify a 4-channel private fixture; log guild-total aggregates only.

**2g. §2.4's honest-limits paragraph is good.** It names crossings and snapshot coarseness as the real lever and sets a concrete failure condition. That's the strongest part of §2 - it's what makes 2b worth fixing rather than replacing.

## 3. napi/Node boundary risks

**3a. TSFN backpressure (§7) proposes the wrong mitigation.** napi-rs TSFNs run over an unbounded internal queue; "coarser batching" makes the queue larger, not bounded - bigger V8 parse spikes when it fires. Real mitigation is a bounded bridge queue with a stated policy: drop-with-count (surfaced via `getHealth`/`eventsDropped`) or close-on-overflow. The plan already claims core has "bounded queues with drop policies" (§2.1) but the one queue that drives GC safety at the boundary - the bridge - is unbounded by omission. Also under-specified: per-client TSFN (N handles ⇒ N TSFNs) or one shared; register/unref semantics.

**3b. TSFN ref semantics gate M1's smoke, not just production.** If the TSFN is created referenced, the Node process won't exit after close/zero-subscription, breaking "smoke exits 0 token-free" (M1/§6). Requirement: unref when no active subscription, re-ref on subscribe, unref permanently on close/closed-failure. Currently this is an unproblematized implementation detail.

**3c. Handle lifecycle enforcement is oversold to TS types.** §2.2 says TS types "require" explicit close/dispose, and §2.2 simultaneously says types are "compile-time only". Those can't both hold in the strong sense. Numeric handles are right for this design, but they need honest support: class marked `Disposable` (`Symbol.dispose`) so `using` works, a documented leak probe (closed above), and admission that forgetting close leaks the client.

**3d. Typegen drift: fine mechanism, wrong enforcement.** §2.2's "check in and diff in CI" plan catches shape drift post-hoc but leaves the regen policy (who/when/what command) unstated, and relies on hand-written fixture names matching typegen's naming conversions (snake_case→camel, Optionality) without a generation-time assertion. Better: assert exported symbols against the generated output in the M3 test, not against a hand list from M2.

**3e. Version pinning matrices silently omit two axes.** Node engines floor (§7 pins via package-lock) - fine but see 1j for crate versions. Node 20 is EOL as of Apr 2026 (plan dated Oct 5 2026); matrix 20/22/24 omits maintenance-LTS 22 while testing an EOL runtime. Recommend 20/22/24, or 22/24 with a documented "20 unsupported" note - currently the stated matrix contradicts "engines >=20 is a floor, not a pin."

**3f. Packaging/prebuild story absent.** `npm pack dry run` (M7) implies a publish-shaped package. That requires per-platform prebuilds (optionalDependencies per triple), a build-from-source fallback, and at least one target-platform decision (linux/mac/win; arm). Nothing in §4 or §7 mentions it. Either scope down to "workstation experiment only" or schedule the packaging milestone.

## 4. Test-methodology holes (beyond measurement validity above)

**4a. Mock injection point is asserted, never specified.** §1.4 and §5.1 say "scripted gateway/REST drive the napi functions in-process," but the binding (§3) is a public surface with no test knob. How does a test show the client use the mock instead of real Discord? Add a test-only constructor path (core already has a scripted-gateway capability) or an env-var opt-in test hook; until then M3's "functions live against mocks" has no mechanism.

**4b. Error-path coverage in the mock suite is narrower than the surface.** §5.1 asserts snapshot shapes, error strings, disposal, no-token-leak. Missing: the `0→1` clamp rule asserted in §3; login timeout 30s discipline; "shard down"-family rejection post-close; reconnect/disconnect behavior at the boundary (does the consumer see a disconnect event, or silence?); mid-stream failure of `streamPages`. If core has these paths but the boundary doesn't test their crossing, the parity claim is untested exactly where binding diverges.

**4c. Sharding is in the API but absent from the test plan.** `createClient({ ..., sharding })` (§3) will have real validation/handshake behavior; §5.1's scripted gateway (READY 1 guild/4 channels) doesn't cover it. Either test it or declare it untested-for-now in §3's out-of-scope list (currently that list only names modal/autocomplete/chunk/voice/lag/guards).

**4d. Live harness (§5.3, 1.4) rules are decent but incomplete on two points.** No statement that M6 runs only locally and never in CI (implied by "env token only," worth an explicit line since the mock suite is unambiguously token-free in CI). And redaction "never IDs that identify the fixture beyond counts" still permits per-channel counts as noted in 2f; tighten to aggregate-only.

## 5. Plain disagreements (wrong or mis-scoped, not just missing)

1. **§3 "iterables"** - remove or defend against the plan's own rare-crossings rule.
2. **§7 "mitigation is coarser batching"** - wrong lever; replace with bounded queue + stated drop/close policy.
3. **§2.4 hard-fail vs §5.2 warn-first** - pick one; they currently contradict.
4. **§2.4's metric** - not computable with §5.2's stated metrics; add allocation sampling or redefine in retention terms.
5. **M2/M3 ordering** - typegen before the surface exists doesn't work; stub or reorder.
6. **M4/M5 ordering** - measurement before a pinned correctness suite risks invalidating the numbers; behavioral suite first.
7. **§3 "rejects with the typed error string"** - must become Error-instance semantics everywhere before implementation inherits the prose.
8. **§7 CI matrix** - testing EOL Node 20 while omitting 22 is backwards; revise.
9. **§1.1 grep scope** - as written the "must print nothing" gate is unpassable once node_modules exists; needs named exclusions.
10. **§1.5 gates** - node/ Rust is currently ungated; the §7 "node CI job" needs an explicit gate list, and 1.5 should name it as "where it applies."
11. **§3 out-of-scope list** - sharding should either be tested (4c) or explicitly moved into the out-of-scope list.

## Bottom line

The GC thesis (state lives in Rust, JS holds droppable snapshots, crossings are the enemy) is well-framed, and §2.4's pre-declared failure mode is the plan's best feature. But the plan needs, before M1: a subscription mechanism, an executor ownership decision, close-drain semantics, an allocation-rate metric (or 2.4 rewritten), Error-object semantics in prose, TSFN unref/ref as a requirement, node-crate gates added to 1.5's node-CI content, pin policy for crate versions and node Cargo.lock, and a 20/22/24 or 22/24 matrix. Each is small; together they close the gaps between an ambitious headline and what §5 can actually measure.
