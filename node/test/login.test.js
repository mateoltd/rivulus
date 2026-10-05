// Login + getters against mocks. Mock secrets only.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const binding = require("../index.js");
const { SECRET, message, setup, teardown } = require("./helpers/fixture");

test("login reaches READY against the scripted gateway", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  const health = binding.getHealth(ctx.handle);
  assert.equal(health.ready, true);
  assert.equal(health.shards, 1);
  await teardown(ctx);
  assert.equal(binding.testLiveCount(), 0);
});

test("getters read without network", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  const rows = binding.getLatencies(ctx.handle);
  assert.equal(rows.length, 1);
  assert.equal(typeof binding.getUptimeMs(ctx.handle), "number");
  const stats = binding.getCacheStats(ctx.handle);
  assert.equal(stats.hitRatio, 1);
  await teardown(ctx);
});

test("unknown handle rejects everywhere", async () => {
  const bad = 999999;
  await assert.rejects(binding.login(bad), /unknown-handle/);
  await assert.rejects(binding.fetchPage(bad, "3", 5), /unknown-handle/);
  assert.throws(() => binding.getHealth(bad), /unknown-handle/);
  await assert.rejects(binding.close(bad), /unknown-handle/);
});

test("bad ids reject with typed strings", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  await assert.rejects(
    binding.fetchPage(ctx.handle, "abc", 5),
    /bad-id: channel/
  );
  await assert.rejects(
    binding.fetchPage(ctx.handle, "3", Number.NaN),
    /bad-id: limit/
  );
  await assert.rejects(binding.fetchPage(ctx.handle, "0", 5), /bad-id: channel/);
  await teardown(ctx);
});

test("bad sharding option is rejected, not silently ignored", () => {
  assert.throws(
    () =>
      binding.createClient({
        token: SECRET,
        intents: 513,
        cache: "balanced",
        sharding: "manual",
      }),
    /bad-sharding/
  );
  assert.equal(binding.testLiveCount(), 0);
});
