# Upstream Ott test suite (vendored)

This directory contains a vendored copy of the **upstream Ott** test suite.

- Upstream repository: https://github.com/ott-lang/ott
- Vendored from commit: `0639ce49892252e0b53e84aa5b0c35b02785913e` (`0639ce4`)
- Source path in upstream: `tests/`

## License

The upstream Ott project is distributed under a BSD-style license.

See: [`LICENCE`](./LICENCE)

## Notes

- We keep the upstream `tests/` structure but only vendor `*.ott` inputs (non-`.ott` helper/output files are intentionally omitted).
- Our CI/regression tests run `ott-core` parsing + checking on all `*.ott` files under `tests/`.
- `test7.ott` is treated as a **known negative test** (it fails in upstream as well) and is expected to fail.
