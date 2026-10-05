// M2 contract check: generated index.d.ts must match every contract item
// tagged Status M1 or earlier, and export nothing else. Fails on drift in
// either direction. No token, no network.
const fs = require("node:fs");
const path = require("node:path");

const root = __dirname;
const contract = fs.readFileSync(path.join(root, "contract.md"), "utf8");
const dts = fs.readFileSync(path.join(root, "index.d.ts"), "utf8");

const failures = [];

// Collect `### `sig`` headings plus their Status tag. Every tagged item
// (M1 and M3) is enforced: the generated file must export it, and export
// nothing else.
const expected = [];
const lines = contract.split("\n");
let current = null;
for (const line of lines) {
  const head = line.match(/^### `(.+?)`/);
  if (head) {
    current = { sig: head[1], status: null };
    continue;
  }
  if (current && current.status === null) {
    const tag = line.match(/Status: (M\d)/);
    if (tag) {
      current.status = tag[1];
      expected.push(current);
      current = null;
    }
  }
  if (/^## /.test(line)) {
    current = null;
  }
}

// Export name + sync/async from a signature. Uses the LAST "):" split so
// nested parens (callback params) parse correctly.
function parseSig(sig) {
  const open = sig.indexOf("(");
  const sep = sig.lastIndexOf("):");
  if (open < 1 || sep < 0) {
    return null;
  }
  return { name: sig.slice(0, open), ret: sig.slice(sep + 2).trim() };
}

// Declared exports in the generated file.
const declared = new Map();
for (const m of dts.matchAll(/^export declare function ([A-Za-z0-9_]+)\((.*?)\):\s*(.+?)\s*$/gm)) {
  declared.set(m[1], { args: m[2], ret: m[3] });
}

const seen = new Set();
for (const item of expected) {
  const parsed = parseSig(item.sig);
  if (!parsed) {
    failures.push(`contract signature not parseable: ${item.sig}`);
    continue;
  }
  seen.add(parsed.name);
  const got = declared.get(parsed.name);
  if (!got) {
    failures.push(`missing export in index.d.ts: ${parsed.name}`);
    continue;
  }
  const wantAsync = parsed.ret.startsWith("Promise");
  const gotAsync = got.ret.startsWith("Promise");
  if (wantAsync !== gotAsync) {
    failures.push(`sync/async mismatch for ${parsed.name}: contract says ${parsed.ret}, d.ts has ${got.ret}`);
  }
}

for (const name of declared.keys()) {
  if (!seen.has(name)) {
    failures.push(`index.d.ts exports ${name} with no contract entry`);
  }
}

if (failures.length > 0) {
  for (const f of failures) {
    console.error(`contract-diff: ${f}`);
  }
  process.exit(1);
}
console.log(`contract-diff: ok (${seen.size} exports match)`);
