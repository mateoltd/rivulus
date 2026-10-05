// Socket scope: every URL in the mock test tree must be loopback.
// This guards the mock-first premise mechanically: no test may point the
// binding (or a mock) at a non-local host. bench/ harnesses are exempt on
// purpose: they are documented local-only live tools, so this scan covers
// test/ only.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

function jsFiles(dir, out) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules") {
        continue;
      }
      jsFiles(full, out);
    } else if (entry.name.endsWith(".js")) {
      out.push(full);
    }
  }
  return out;
}

test("all URLs in mock tests are loopback", () => {
  const files = jsFiles(path.join(__dirname), []);
  const offenders = [];
  const urlPattern = /(?:https?|wss?):\/\/([^/:)"'\s]+)/g;
  for (const file of files) {
    const text = fs.readFileSync(file, "utf8");
    let match;
    while ((match = urlPattern.exec(text)) !== null) {
      const host = match[1];
      if (host !== "127.0.0.1" && host !== "localhost") {
        offenders.push(`${file}: ${match[0]}`);
      }
    }
  }
  assert.deepEqual(offenders, []);
});
