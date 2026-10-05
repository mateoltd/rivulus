# Rivulus — Plans Index

> Discord.js, but Better — in Rust. Target: discord.js-level capabilities at ~80% less RAM.

## Naming rule (locked, D1)
Cargo package = `rivulus-<short>` (`rivulus-gateway`), Rust lib/import = short word
(`gateway`, `rest`, `model`, `core`, `cache`, `standby`, `interactions`, `framework`, `voice`).
Facade `rivulus` re-exports: `use rivulus::gateway::Shard`. Never write package-prefixed imports (hyphenated `rivulus-` package path) or `rivulus::rivulus` in docs/code. Crate dirs: `crates/{common,model,gateway,rest,cache,standby,interactions,framework,voice,rivulus}`.

## Locked decisions — read first
`09-decisions-locked.md`: HTTP=reqwest, JSON=serde_json, cache=dashmap6, MSRV=1.75, voice=deferred.
Do not re-debate without RFC.

## How to read these plans
| File | Contents |
|------|----------|
| `01-vision-scope-success-criteria.md` | Objectives, non-goals, success gates, constraints |
| `02-architecture-workspace.md` | Workspace, crates, features, dependency policy, public API |
| `03-discordjs-parity-matrix.md` | discord.js → rivulus mapping, coverage matrix, deltas |
| `04-protocol-deep-dives.md` | Gateway / REST / Cache / Interactions / Voice normative specs |
| `05-performance-benchmarking.md` | Budgets, alloc rules, bench harness vs discord.js |
| `06-reliability-safety-observability.md` | Errors, resume, ratelimits, backoff, logging/metrics |
| `07-testing-ci-docs-release.md` | Test pyramid, CI, docs, examples, release train |
| `08-agentic-execution-plan.md` | Phases P0–P12, task DAG, acceptance, agent playbook |
| `09-decisions-locked.md` | Locked: namespaces, HTTP, JSON, cache, MSRV, voice-deferred |
| `10-review-log.md` | Independent + SpaceBunny XHigh review findings and disposition |

## Execution order for agents
1. Read `09` → `01` → `02` → `03` first (locks + scope + shape + parity).
2. Implement per `08` phases in order; consult `04/05/06` as normative refs.
3. No phase is "done" without its `08` acceptance checks + `07` quality gates.
4. Perf work (`05`) runs in parallel from P2 onward, enforced at P10.

## Global invariants (non-negotiable)
- Workspace at `/home/zero/lab/rivulus.rs`, edition 2021, MSRV 1.75, `tokio` only async runtime.
- `serde_json` default via `common::json` shim. No second HTTP/WS stack.
- No `unwrap`/`expect` in library `src/` (tests allowed); all errors typed via `common::Error`.
- Every public API documented with example; every gateway/rest path has test.
- Optional functionality behind Cargo features costs ~zero when disabled.
- Short namespace imports only (`gateway::`, `rest::`, `model::`, `common::`, …).
