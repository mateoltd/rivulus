// Thin consumer run end to end against mocks (plan 2.3 pattern).
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { execFile } = require("node:child_process");
const path = require("node:path");
const { json, startMockRest, stop: stopRest } = require("./helpers/mock-rest");
const {
  startMockGateway,
  stop: stopGateway,
} = require("./helpers/mock-gateway");
const { message } = require("./helpers/fixture");

test("example reads snapshots and drops them", async () => {
  const rest = await startMockRest([
    { prefix: "/", next: (() => {
      let i = 0;
      return () => json(200, i++ === 0 ? [message("4")] : []);
    })() },
  ]);
  const gateway = await startMockGateway({});
  const output = await new Promise((resolve, reject) => {
    execFile(
      process.execPath,
      [path.join(__dirname, "..", "example.js")],
      {
        env: {
          ...process.env,
          MOCK_REST_BASE: `http://127.0.0.1:${rest.port}`,
          MOCK_GATEWAY_BASE: `ws://127.0.0.1:${gateway.port}/`,
          CHANNEL_ID: "3",
        },
        timeout: 30000,
      },
      (error, stdout, stderr) => {
        if (error) {
          reject(new Error(`exit!=0 stdout=${stdout} stderr=${stderr}`));
          return;
        }
        resolve(stdout);
      }
    );
  });
  assert.match(output, /messages=1/);
  assert.match(output, /ready=true/);
  await stopRest(rest.server);
  await stopGateway(gateway.server);
});
