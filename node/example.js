// Thin consumer pattern (plan sections 2.3, M3).
//
// Reads snapshots and drops them: no per-message caches, no retained
// closures over pages. Endpoints come from the environment so the same
// file runs against the M4 mocks (MOCK_REST_BASE, MOCK_GATEWAY_BASE) or,
// with a real token in DISCORD_TOKEN, against the quiet live guild
// (read-only: login, one fetchPage, health). Defaults point at nothing;
// without env it prints usage and exits 2 (never attempts production).

const binding = require("./index.js");

async function main() {
  const useMock = process.env.MOCK_REST_BASE && process.env.MOCK_GATEWAY_BASE;
  const token = useMock ? "mock-token" : process.env.DISCORD_TOKEN;
  if (!token) {
    console.error("usage: MOCK_REST_BASE+MOCK_GATEWAY_BASE or DISCORD_TOKEN");
    process.exit(2);
  }
  const create = useMock ? binding.createTestClient : binding.createClient;
  const args = useMock
    ? [
        {
          token,
          intents: 513,
          cache: "balanced",
          sharding: "auto",
        },
        process.env.MOCK_REST_BASE,
        process.env.MOCK_GATEWAY_BASE,
      ]
    : [
        {
          token,
          intents: 513,
          cache: "balanced",
          sharding: "auto",
        },
      ];
  const handle = create(...args);
  await binding.login(handle);
  const channelId = process.env.CHANNEL_ID || "3";
  const page = await binding.fetchPage(handle, channelId, 50);
  let seen = 0;
  for (const message of page) {
    seen += 1;
    void message.content;
  }
  const health = binding.getHealth(handle);
  console.log(
    `example: messages=${seen} ready=${health.ready} dropped=${health.eventsDropped}`
  );
  await binding.close(handle);
}

main().then(
  () => {},
  (error) => {
    console.error(`example failed: ${error && error.message}`);
    process.exit(1);
  }
);
