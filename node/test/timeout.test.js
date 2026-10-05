// Timeout discipline, mid-stream failure, disconnect visibility.
// Mock only. The 30s login-timeout test is slow on purpose: it proves the
// READY discipline instead of asserting it exists.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const binding = require("../index.js");
const { message, setup, teardown } = require("./helpers/fixture");

test(
  "login against a silent gateway rejects at 30s READY discipline",
  { timeout: 60000 },
  async (t) => {
    const ctx = await setup([], { silent: true });
    t.after(() => teardown(ctx));
    const start = Date.now();
    await assert.rejects(binding.login(ctx.handle), /timeout/);
    const elapsed = Date.now() - start;
    assert.ok(
      elapsed >= 29000 && elapsed < 45000,
      `expected ~30s discipline, took ${elapsed}ms`
    );
  }
);

test("mid-stream REST failure rejects the stream", async (t) => {
  const { json, startMockRest, stop: stopRest } = require("./helpers/mock-rest");
  const {
    startMockGateway,
    stop: stopGateway,
  } = require("./helpers/mock-gateway");
  let n = 0;
  const rest = await startMockRest([
    {
      prefix: "/",
      next: () =>
        n++ === 0
          ? json(200, [message("4")[0]])
          : json(500, { code: 0, message: "boom" }),
    },
  ]);
  const gateway = await startMockGateway({});
  const handle = binding.createTestClient(
    { token: "mock", intents: 513, cache: "balanced", sharding: "auto" },
    `http://127.0.0.1:${rest.port}`,
    `ws://127.0.0.1:${gateway.port}/`
  );
  t.after(async () => {
    await binding.close(handle).catch(() => {});
    await stopRest(rest.server);
    await stopGateway(gateway.server);
  });
  await binding.login(handle);
  // First page full (limit 1), second page is HTTP 500: the stream must
  // reject with a typed error, never hang or return partial silence.
  await assert.rejects(binding.streamPages(handle, "3", 1));
});

test("gateway drop stays usable for REST (documented silence)", async (t) => {
  const ctx = await setup([[message("4")]], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  // Drop every server-side connection: the supervisor reconnects, REST
  // keeps working, and the binding emits no disconnect event (silence is
  // the documented behavior; surface validation is out of scope).
  for (const client of ctx.gateway.server.clients) {
    client.terminate();
  }
  await new Promise((resolve) => setTimeout(resolve, 1500));
  const page = await binding.fetchPage(ctx.handle, "3", 50);
  assert.equal(page.length, 1);
  const health = binding.getHealth(ctx.handle);
  assert.equal(health.ready, true);
});
