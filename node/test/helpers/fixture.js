// Shared fixture: mock REST plus mock gateway plus a test client.
// Mock secrets only; never production, never a real token.
const binding = require("../../index.js");
const { json, startMockRest, stop: stopRest } = require("./mock-rest");
const {
  startMockGateway,
  stop: stopGateway,
} = require("./mock-gateway");

const SECRET = "MOCK-SECRET-XYZ-9";

function message(id) {
  return {
    id: String(id),
    channel_id: "3",
    author: { id: "2", username: "u" },
    content: "hi",
    timestamp: "2024-01-01T00:00:00Z",
  };
}

function queued(bodies) {
  let i = 0;
  return () => {
    const body = i < bodies.length ? bodies[i] : {};
    i += 1;
    return json(200, body);
  };
}

async function setup(restBodies, gatewayOptions) {
  const rest = await startMockRest([
    {
      prefix: "/",
      next: queued(restBodies),
    },
  ]);
  const gateway = await startMockGateway(gatewayOptions);
  const handle = binding.createTestClient(
    { token: SECRET, intents: 513, cache: "balanced", sharding: "auto" },
    `http://127.0.0.1:${rest.port}`,
    `ws://127.0.0.1:${gateway.port}/`
  );
  return { handle, rest, gateway };
}

async function teardown({ handle, rest, gateway }) {
  await binding.close(handle).catch(() => {});
  await stopRest(rest.server);
  await stopGateway(gateway.server);
}

module.exports = { SECRET, message, setup, teardown };
