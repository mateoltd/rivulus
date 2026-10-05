# rivulus-node surface contract (M2, hand-written)

This file is the reviewable source of truth for the binding surface. The
generated `index.d.ts` must match every item tagged `implemented` below;
`check-contract.js` enforces that and fails on drift in either direction
(missing export, or export with no contract entry). Items tagged `planned`
are docs for M3 and are not checked.

Status tags: `M1` means implemented now, `M3` means planned.

## Handles and lifecycle

- Handles are monotonic `u64` ids crossed as JS numbers, never recycled.
  ABA is impossible by construction. Status: M3.
- Unknown ids reject with an `Error` whose message is exactly
  `unknown-handle`. Status: M3.
- `close` is async: it drops the bridge queue, releases the threadsafe
  function, then resolves. Post-close callbacks are impossible. In-flight
  `login`/`fetchPage` promises reject with the close error. Double `close`
  succeeds trivially. Status: M3.
- A test-only live-handle counter (never in `.d.ts`) is asserted zero at
  process exit. Status: M3.
- No napi `External` or class finalizers: close is explicit and
  deterministic. Status: M3 (design rule, no export).

## Functions

### `version(): string` (sync, no handle)

Returns the `rivulus-node` crate version. Status: M1.

### `createClient(options): number` (sync)

- `options.token: string` (kept in Rust as SecretString, never readable
  back; necessarily present in the V8 heap at entry, see plan 2.1).
- `options.intents: number` (bitfield value).
- `options.cache: "minimal" | "balanced" | "full"`.
- `options.sharding: "auto"` (any other value rejects with
  `bad-sharding`; multi-shard behavior is untested-for-now).
- Returns the numeric handle. Status: M3.

### `subscribe(handle: number, kinds: string[]): void` (sync)

### `unsubscribe(handle: number, kinds: string[]): void` (sync)

First subscribe creates the per-client threadsafe function; zero active
subscriptions unrefs it; close releases it permanently. Status: M3.

### `login(handle: number): Promise<void>` (async)

Resolves after 30s READY discipline. Rejects with an `Error` whose
`.message` is exactly one of: `timeout: READY timeout`,
`unauthorized: check token`, `network: ...` (rest text follows the
prefix), `unknown-handle`, `client-closed`. Status: M3.

### `fetchPage(handle: number, channelId: string, limit: number): Promise<Message[]>` (async)

Clamp: `0` maps to `1`. Rejects with the same error family as `login`.
Status: M3.

### `streamPages(handle: number, channelId: string): AsyncIterable<Page>` (async iteration)

Each `Page` is one coarse pre-shaped `Message[]` crossing. The binding
holds the paging cursor (`after`, mirroring core); JS iterates with no
cursor math. Dropping iteration via explicit `return()` frees the Rust
allocation. Status: M3.

### Getters (all sync, all take `handle: number`)

- `getHealth(handle): { ready: boolean, shards: number, guildsCached: number, eventsDropped: number }`
- `getLatencies(handle): { shard: number, total: number, latencyMs: number }[]`
- `getUptimeMs(handle): number`
- `getCacheStats(handle): { hitRatio: number, guilds: number, channels: number, messages: number }`

Status: M3.

### `verifyWebhook(publicKeyHex: string, timestamp: string, body: Buffer, sigHex: string): void` (sync, pure)

Throws on invalid signature or malformed hex (hex-decode failure is its
own error, not a silent false). Concat rule `timestamp_bytes || body_bytes`.
Bytes only: no string overload (UTF-16 re-encoding cannot be byte-exact).
Status: M3.

### `close(handle: number): Promise<void>` (async)

See lifecycle above. Status: M3.

## Snapshot shapes (pinned)

```ts
interface Message {
  id: string;
  channelId: string;
  authorId: string;
  content: string;
  timestamp: string;
}
type Page = Message[];
```

IDs cross as strings, always (u64 does not fit in a double). Status: M3
(shape docs; enforced by tests from M3 on).
