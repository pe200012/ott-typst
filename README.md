# ott (next‑gen)

Rust + WASM + Typst reimplementation of the Ott toolchain.

## What you can do today

- Parse a large subset of Ott’s `.ott` DSL (tested against upstream Ott `tests/*.ott`, except known negative tests).
- Run basic semantic checks (duplicate grammar roots, subrules references, duplicate rule names).
- Render grammars and inference rules in Typst via a WASM plugin.

## Repository layout

- `crates/ott-core/` — parser + basic checks
- `crates/ott-render/` — Typst render IR + CBOR serialization
- `crates/ott-cli/` — CLI (`check`, `render-json`, `render-cbor`)
- `crates/ott-wasm/` — WASM plugin (`parse_rules`)
- `typst/ott.typ` — Typst-side renderer (uses `@preview/curryst`)
- `demo.typ` — root demo document

## Build & run the Typst demo

Prerequisites:

- Rust toolchain with `wasm32-unknown-unknown` target
- Typst `>= 0.14` (uses built-in `cbor(...)`)

Build the WASM plugin and copy it into the Typst package directory:

```bash
cargo build -p ott-wasm --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/ott_wasm.wasm typst/plugins/ott.wasm
```

Compile the demo:

```bash
typst compile --root . demo.typ demo.pdf
```

## Use in your own Typst document

```typst
#import "typst/ott.typ": render

#render(read("path/to/spec.ott"))
```

Notes:

- Current rendering uses `raw(...)` for premises/conclusions (no structured math AST yet).
- Filter-mode (`[[...]]`) and proof-assistant backends are not implemented yet.

## Design / plan

See: `docs/plans/2026-03-04-ott-typst-rust-nextgen-design.md`.
