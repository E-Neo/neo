# WASI WIT sources

`neo.wit` is this project's root world. `deps/` contains an unmodified copy of
the official WASI 0.2.9 WIT packages, used only so `neo build` can resolve and
embed the component metadata without a network or SDK dependency.

## Provenance

Vendored from the Rust `wasm32-wasip2` target's bundled sources:

- Crate: `wasip2-1.0.3+wasi-0.2.9`
- Path: `library/vendor/wasip2-1.0.3+wasi-0.2.9/wit/deps/*.wit`
- Upstream package versions: `wasi:cli@0.2.9`, `wasi:clocks@0.2.9`,
  `wasi:filesystem@0.2.9`, `wasi:http@0.2.9`, `wasi:io@0.2.9`,
  `wasi:random@0.2.9`, `wasi:sockets@0.2.9`

## Checksums

```
91aebf406712e618d037fa153b270476541d48d890b4ad6b111ec97b40d73194  cli.wit
654a3b5e8b065202334de477828444dca099e576660b6a069c6aaf64685bf68f  clocks.wit
4065ba3e5d45ac9cdcfb4bbc31774424d24c453abccf1c9421414ff45f8459b9  filesystem.wit
873455584340b0e0aa801b842a72d86a67206825bf2d8fc6bca870da6e6ff8f6  http.wit
80957ac766c6b7eb743ce237a8f92700e1a0b007a1856794ef1972b61ddfce50  io.wit
4b3229e0ec18cc3fe71f824f855d9cf04809ec8a8d383f088671ca654f7fb885  random.wit
d19af1d835606a576aace12d24a3699aa6f150e22084bb8d880e285c6257786c  sockets.wit
```

When updating WASI, replace every file together, refresh these checksums, and
bump the `@0.2.x` versions in `neo.wit` and `src/codegen.rs` imports/exports.

The World only imports `wasi:cli/stdout` and `wasi:io/streams`, but the parser
resolves the whole dependency graph, so all seven files are required.
