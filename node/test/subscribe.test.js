// Listener delivery in a real JS env (deferred from M3). Mock only.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const binding = require("../index.js");
const { message, setup, teardown } = require("./helpers/fixture");

function dispatchMessage(id) {
  return {
    op: 0,
    t: "MESSAGE_CREATE",
    s: 2,
    d: message(id),
  };
}

test("subscribed listener receives JSON batches", async (t) => {
  const ctx = await setup([[]], {
    extraFrames: [dispatchMessage("7")],
  });
  t.after(() => teardown(ctx));
  const batches = [];
  // Subscribe before login: the dispatch lands after READY, long after the
  // pump is running, so no race is possible. Listener is error-first
  // (null on delivery) per the napi CalleeHandled strategy.
  binding.subscribe(ctx.handle, ["MESSAGE_CREATE"], (_error, batch) => {
    batches.push(batch);
  });
  await binding.login(ctx.handle);
  await new Promise((resolve, reject) => {
    const deadline = setTimeout(
      () => reject(new Error("no batch arrived")),
      5000
    );
    const poll = setInterval(() => {
      if (batches.length > 0) {
        clearTimeout(deadline);
        clearInterval(poll);
        resolve();
      }
    }, 25);
  });
  const parsed = JSON.parse(batches[0]);
  assert.ok(Array.isArray(parsed));
  assert.equal(parsed[0].kind, "MESSAGE_CREATE");
  assert.equal(parsed[0].event.id, "7");
  binding.unsubscribe(ctx.handle);
  await teardown(ctx);
  assert.equal(binding.testLiveCount(), 0);
});

test("dropped counter stays zero on a quiet subscribed client", async (t) => {
  const ctx = await setup([[]], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  binding.subscribe(ctx.handle, ["MESSAGE_CREATE"], () => {});
  await new Promise((resolve) => setTimeout(resolve, 300));
  const health = binding.getHealth(ctx.handle);
  assert.equal(health.eventsDropped, 0);
  await teardown(ctx);
});
