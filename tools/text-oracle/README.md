# text-oracle

Runs Excalidraw's own `textWrapping.ts` and `textMeasurements.ts`, unmodified, under Node's
type stripping. Nothing to install: `hooks.mjs` points `@excalidraw/common` at
`common-shim.ts`, a small file with the few constants and helpers those two files import.

```sh
# from the host repo; runs in mcr.microsoft.com/playwright:v1.63.0-noble
make oracle-fixtures
# or by hand, from this directory
EXCALIDRAW_DIR=/path/to/excalidraw ORACLE_SHA=<pin> node --import ./register.mjs generate.mjs [seed] [count]
```

The checkout must be at the pin. Before writing anything, the script checks its output
against the oracle's own unit tests plus the CJK sentence cases, and exits 2 if any
fails. It writes:

- `crates/draw-engine/tests/fixtures/text-wrap.oracle.json`: wrap cases under the
  `jsdom`, `table` and `kern` width models, the offsets for each case, and the
  `parseTokens` output for each line. `tests/ci_text_wrap_oracle.rs` replays it.
- `crates/draw-engine/src/text/unicode.rs`: the Unicode classes the oracle's regexes use,
  and the NFC tables. They follow the Node that ran the script, which the header records.

Output is deterministic. Regenerate only when the pin moves, never to turn a red test
green.
