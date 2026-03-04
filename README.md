# ott (next‑gen)

Rust + WASM + Typst reimplementation of the Ott toolchain.

## What you can do today

- Parse a large subset of Ott’s `.ott` DSL (tested against upstream Ott `tests/*.ott`, except known negative tests).
- Run basic semantic checks (duplicate grammar roots, subrules references, duplicate rule names).
- Render grammars and inference rules in Typst via a WASM plugin.
- Parse and validate object-language terms/snippets in Typst from a loaded Ott spec (`ott-file` → `#ott[...]`).

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

````typst
#import "typst/ott.typ": render, ott-file

// Render grammar + inference rules from a spec
#render(read("path/to/spec.ott"))

// Build a term parser (choose a start nonterminal/root)
#let ott = ott-file(read("path/to/spec.ott"), root: "t")

// Parse + typeset terms
#ott[x]
#ott[`\x.x`]
$ #ott[`[x|->x]x`] $
````

Notes:

- Term parsing returns `raw(...)` content for now (monospace, safe to embed in markup or math).
- Filter-mode (`[[...]]`) is not implemented; use `ott-file(...)` + the returned `ott[...]` function instead.
- Proof-assistant backends are not implemented yet.

## Design / plan

See: `docs/plans/2026-03-04-ott-typst-rust-nextgen-design.md`.
