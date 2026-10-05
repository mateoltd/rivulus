// Lifecycle: close paths, mid-flight close, leak probe. Mock only.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const binding = require("../index.js");
const { setup, teardown } = require("./helpers/fixture");
test("close-then-call rejects, second close is unknown-handle", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  await binding.close(ctx.handle);
  await assert.rejects(binding.fetchPage(ctx.handle, "3", 5));
  await assert.rejects(binding.close(ctx.handle), /unknown-handle/);
  await teardown(ctx);
  assert.equal(binding.testLiveCount(), 0);
});

test("close during pending login rejects the login", async (t) => {
  const ctx = await setup([], { silent: true });
  t.after(() => teardown(ctx));
  const pending = binding.login(ctx.handle);
  await new Promise((resolve) => setTimeout(resolve, 200));
  await binding.close(ctx.handle);
  await assert.rejects(pending);
  await teardown(ctx);
  assert.equal(binding.testLiveCount(), 0);
});
