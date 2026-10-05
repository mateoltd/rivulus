# rivulus-node (M1 scaffold)

Node.js binding over the rivulus core. Standalone crate: not a workspace
member, so the 1.75 MSRV job never builds it. Builds on stable only.

## Pins (plan section 4)

- `napi =2.16.17` (newest napi 2.x resolving here; declares rust 1.65)
- `napi-derive =2.16.13` (newest 2.16.x published; satisfies napi `^2.10.1`)
- `napi-build =2.2.4` (era match for napi 2.16)
- `@napi-rs/cli =2.18.4` (exact dev pin)
- Crate MSRV `1.85` (max of the above; stable-only by layout, never CI
  gated on 1.75)

Regenerate bindings plus types: `npm run build` (wraps
`napi build --platform`; artifacts `index.js` plus `index.d.ts` are
checked in, `*.node` binaries are not).

## Layout

- `Cargo.toml` plus `build.rs` (`napi_build::setup()`) plus `src/`
- `src/runtime.rs`: dedicated single-thread tokio runtime on its own OS
  thread (plan section 2.2 decision). M3 entry points submit here.
- `src/lib.rs`: `version()` smoke surface only in M1.

## Why no `forbid(unsafe_code)` here

`napi-derive` expands to `unsafe extern "C"` entry points, so a
crate-level forbid cannot compile. Hand-written code uses no `unsafe`
blocks; the M1 gate greps `src/` for them.
