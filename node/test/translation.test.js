// Translation edges: the boundary mistranslations most likely to break.
// i64/string IDs, limit edges, stats typing. Mock only.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const binding = require("../index.js");
const { message, setup, teardown } = require("./helpers/fixture");

test("u64 snowflakes cross as exact strings past 2^53", async (t) => {
  const big = "18446744073709551615";
  const ctx = await setup(
    [
      [
        {
          id: big,
          channel_id: "3",
          author: { id: "2", username: "u" },
          content: "hi",
          timestamp: "2024-01-01T00:00:00Z",
        },
      ],
    ],
    {}
  );
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  const page = await binding.fetchPage(ctx.handle, "3", 50);
  assert.equal(page.length, 1);
  assert.equal(page[0].id, big);
  assert.equal(typeof page[0].id, "string");
});

test("limit edges reject with typed strings", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  await assert.rejects(binding.fetchPage(ctx.handle, "3", 256), /bad-id: limit/);
  await assert.rejects(binding.fetchPage(ctx.handle, "3", -1), /bad-id: limit/);
  await assert.rejects(binding.fetchPage(ctx.handle, "3", 1.5), /bad-id: limit/);
  await assert.rejects(
    binding.fetchPage(ctx.handle, "3", Number.POSITIVE_INFINITY),
    /bad-id: limit/
  );
});

test("stats cross with number typing", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  const rows = binding.getLatencies(ctx.handle);
  for (const row of rows) {
    assert.equal(typeof row.shard, "number");
    assert.equal(typeof row.total, "number");
    assert.equal(typeof row.latencyMs, "number");
  }
  const stats = binding.getCacheStats(ctx.handle);
  assert.equal(typeof stats.hitRatio, "number");
  assert.equal(typeof stats.guilds, "number");
  assert.equal(typeof stats.channels, "number");
  assert.equal(typeof stats.messages, "number");
  assert.equal(typeof binding.getUptimeMs(ctx.handle), "number");
});

test("handles increase monotonically across close", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  const first = ctx.handle;
  await binding.close(first);
  const second = binding.createTestClient(
    { token: "mock", intents: 513, cache: "balanced", sharding: "auto" },
    "http://127.0.0.1:1",
    "ws://127.0.0.1:1/"
  );
  t.after(() => binding.close(second).catch(() => {}));
  assert.ok(second > first, "no id reuse after close");
});
