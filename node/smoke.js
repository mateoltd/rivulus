// M1 smoke: loads the binding, prints its version, exits 0. No token,
// no network, no Discord. Any failure exits non-zero with the error text.
const binding = require("./index.js");

const version = binding.version();
if (typeof version !== "string" || version.length === 0) {
  console.error("smoke: version() did not return a non-empty string");
  process.exit(1);
}
console.log(`smoke: rivulus-node ${version} ok`);
