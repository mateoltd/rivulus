// GC measurement harness (plan M5). Mock only; no token anywhere.
//
// PRE-REGISTERED BANDS (fixed before the first run; verdict compares
// medians against these, not against each other post hoc):
// - survivors(a) <= 1.2 * survivors(b)   (neither retains per-message state)
// - allocated(a) <= 1.5 * allocated(b)    (parse plus snapshots vs objects)
// - rssDelta is advisory only (tripwire, no gate)
// - canary (c): survivors(c) must exceed 10x survivors(a), else the harness
//   is invalid (rerun, not fail). Arm (d) documents cache cost, no gate.
//
// WHAT EACH METRIC DOES AND DOES NOT CAPTURE:
// - allocated/msg: heap growth over K iterations with no intermediate GC.
//   V8 may still collect young generations mid-loop, so this is a LOWER
//   bound on true allocation, stated honestly. Compared across arms run
//   back to back in one process, same binary, same flags.
// - survivors/msg: heap growth that survives a forced full GC. Exact.
// - rssDelta: process RSS before/after. Noisy; tripwire only.
// GC event counts from --trace-gc are deliberately out of scope for v1:
// trace formats drift across Node 20/22/24 and parsing them is its own
// brittle sub-project. Run with or without --trace-gc; results identical.
//
// FIXED PARAMS: 25 messages per iteration (~180B JSON each, fixed content),
// K=40 iterations per arm (+5 warmup), R=3 repeats, median taken. Pinned
// numbers run on the local Node (recorded in the results file); CI runs
// the harness for rot only (must exit 0, no gating).
//
// ARMS (same mock REST for a/b/c; djs objects for b/d):
// - (a) binding fetchPage, snapshots dropped per iteration.
// - (b) discord.js REST get plus new Message, caches disabled, dropped.
// - (c) binding fetchPage, every page retained (canary).
// - (d) discord.js REST get plus channel.messages._add (default caches).
//
// Run: node --expose-gc bench/gc-measure.js [--quick]

const path = require("node:path");

const QUICK = process.argv.includes("--quick");
const K = QUICK ? 8 : 40;
const WARMUP = QUICK ? 2 : 5;
const REPEATS = QUICK ? 1 : 3;
const PAGE = 25;

function fixtureMessage(i) {
  return {
    id: String(1000 + i),
    channel_id: "3",
    author: { id: "2", username: "u" },
    content: `m${i}-` + "x".repeat(150),
    timestamp: "2024-01-01T00:00:00Z",
  };
}

function fixturePage() {
  const out = [];
  for (let i = 0; i < PAGE; i++) {
    out.push(fixtureMessage(i));
  }
  return out;
}

function heapUsed() {
  return process.memoryUsage().heapUsed;
}

function gcNow() {
  global.gc();
  global.gc();
}

async function main() {
  if (typeof global.gc !== "function") {
    console.error("measure: needs node --expose-gc");
    process.exit(2);
  }
  const { startMockRest, stop } = require("../test/helpers/mock-rest");
  const rest = await startMockRest([
    {
      prefix: "/",
      next: (() => {
        const page = fixturePage();
        const body = JSON.stringify(page);
        return () => ({
          status: 200,
          headers: { "Content-Type": "application/json" },
          body,
        });
      })(),
    },
  ]);
  const base = `http://127.0.0.1:${rest.port}`;
  const results = {};
  for (let r = 0; r < REPEATS; r++) {
    results[`run${r}`] = await runAllArms(base);
  }
  await stop(rest.server);
  const medians = medianArms(results);
  console.log(JSON.stringify({ params: { K, WARMUP, REPEATS, PAGE }, medians }, null, 2));
}

async function runAllArms(base) {
  const binding = require("../index.js");
  const { Client, Options, Message } = require("discord.js");
  const { REST } = require("@discordjs/rest");
  const out = {};
  out.a = await armBinding(binding, base);
  out.b = await armDjsNocache(Client, Options, Message, REST, base);
  out.c = await armBindingRetain(binding, base);
  out.d = await armDjsCached(Client, Message, REST, base);
  return out;
}

async function armBinding(binding, base) {
  const handle = binding.createTestClient(
    { token: "mock", intents: 513, cache: "balanced", sharding: "auto" },
    base,
    "ws://127.0.0.1:1/"
  );
  for (let i = 0; i < WARMUP; i++) {
    await binding.fetchPage(handle, "3", PAGE);
  }
  gcNow();
  const h0 = heapUsed();
  const r0 = process.memoryUsage().rss;
  for (let i = 0; i < K; i++) {
    await binding.fetchPage(handle, "3", PAGE);
  }
  const h1 = heapUsed();
  gcNow();
  const h2 = heapUsed();
  const r1 = process.memoryUsage().rss;
  await binding.close(handle);
  const n = K * PAGE;
  return { alloc: (h1 - h0) / n, surv: (h2 - h0) / n, rss: (r1 - r0) / n };
}

async function armBindingRetain(binding, base) {
  const kept = [];
  const handle = binding.createTestClient(
    { token: "mock", intents: 513, cache: "balanced", sharding: "auto" },
    base,
    "ws://127.0.0.1:1/"
  );
  for (let i = 0; i < WARMUP; i++) {
    await binding.fetchPage(handle, "3", PAGE);
  }
  gcNow();
  const h0 = heapUsed();
  const r0 = process.memoryUsage().rss;
  for (let i = 0; i < K; i++) {
    kept.push(await binding.fetchPage(handle, "3", PAGE));
  }
  const h1 = heapUsed();
  gcNow();
  const h2 = heapUsed();
  const r1 = process.memoryUsage().rss;
  await binding.close(handle);
  const n = K * PAGE;
  void kept.length;
  return { alloc: (h1 - h0) / n, surv: (h2 - h0) / n, rss: (r1 - r0) / n };
}

async function armDjsNocache(Client, Options, Message, REST, base) {
  const client = new Client({
    intents: [],
    makeCache: Options.cacheWithLimits({ MessageManager: 0 }),
  });
  const rest = new REST({ version: "10", api: base }).setToken("mock");
  for (let i = 0; i < WARMUP; i++) {
    const raw = await rest.get("/channels/3/messages", { query: new URLSearchParams({ limit: String(PAGE) }) });
    for (const m of raw) {
      void new Message(client, m);
    }
  }
  gcNow();
  const h0 = heapUsed();
  const r0 = process.memoryUsage().rss;
  for (let i = 0; i < K; i++) {
    const raw = await rest.get("/channels/3/messages", { query: new URLSearchParams({ limit: String(PAGE) }) });
    for (const m of raw) {
      void new Message(client, m);
    }
  }
  const h1 = heapUsed();
  gcNow();
  const h2 = heapUsed();
  const r1 = process.memoryUsage().rss;
  client.destroy();
  const n = K * PAGE;
  return { alloc: (h1 - h0) / n, surv: (h2 - h0) / n, rss: (r1 - r0) / n };
}

async function armDjsCached(Client, Message, REST, base) {
  void Message;
  const client = new Client({ intents: [] });
  const channel = client.channels._add({ id: "3", type: 1 });
  const rest = new REST({ version: "10", api: base }).setToken("mock");
  for (let i = 0; i < WARMUP; i++) {
    const raw = await rest.get("/channels/3/messages", { query: new URLSearchParams({ limit: String(PAGE) }) });
    for (const m of raw) {
      channel.messages._add(m);
    }
  }
  gcNow();
  const h0 = heapUsed();
  const r0 = process.memoryUsage().rss;
  for (let i = 0; i < K; i++) {
    const raw = await rest.get("/channels/3/messages", { query: new URLSearchParams({ limit: String(PAGE) }) });
    for (const m of raw) {
      channel.messages._add({ ...m, id: `${m.id}-${i}` });
    }
  }
  const h1 = heapUsed();
  gcNow();
  const h2 = heapUsed();
  const r1 = process.memoryUsage().rss;
  const size = channel.messages.cache.size;
  client.destroy();
  const n = K * PAGE;
  void size;
  return { alloc: (h1 - h0) / n, surv: (h2 - h0) / n, rss: (r1 - r0) / n };
}

function medianArms(results) {
  const arms = ["a", "b", "c", "d"];
  const metrics = ["alloc", "surv", "rss"];
  const out = {};
  for (const arm of arms) {
    out[arm] = {};
    for (const metric of metrics) {
      const vals = Object.values(results)
        .map((r) => r[arm][metric])
        .sort((x, y) => x - y);
      out[arm][metric] = vals[Math.floor(vals.length / 2)];
    }
  }
  return out;
}

main().then(
  () => {},
  (error) => {
    console.error(`measure failed: ${error && error.message}`);
    process.exit(1);
  }
);
