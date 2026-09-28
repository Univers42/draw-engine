# The faces an exported SVG carries

These are the same bytes, byte for byte, as the ones the web app serves from
`apps/web/static/fonts/` — which are themselves byte-identical to what Excalidraw ships at
the commit this project is held to (`scripts/oracle-sha.txt`,
`packages/excalidraw/fonts/<Family>/<file>`). Every file here is in `assets/fonts/`
because the SVG exporter embeds them, and it embeds them from Rust: BUNNY.md §2 does not
allow the host to name a font, and a host that fetched the file would have to be told which
one to fetch.

They are here twice on purpose and it is a duplication worth naming rather than hiding: the
app needs them to *draw* with, and an exported file needs them to *be* the drawing after it
leaves. One copy would mean the engine reaching into `apps/web` at build time, which is not
a thing the submodule can do.

`crates/draw-engine/src/export/font_face.rs` holds the table that names these files, and a
test that the table and this directory agree is `ci_svg_fonts_bytes.rs`.

## Licences

Unchanged, and the full texts are next to the files: `OFL-1.1.txt` (Virgil, Excalifont,
Nunito, Lilita One), `ComicShanns-MIT.txt`, `Cascadia-LICENSE.txt` (SIL OFL 1.1, the
upstream release file for the version this is). Every one of them permits redistribution
with the licence attached, and every file is shipped **unmodified**, which is what the OFL
Reserved Font Name clauses in "Lilita One" and "Cascadia Code" require.

The reasoning behind each row — read out of the OpenType `name` tables with fontTools, and
where a file did not say enough, out of the upstream project's own licence — is in
`apps/web/static/fonts/LICENSES.md`. It is not repeated here because it is one argument, and
two copies of an argument is how they come to disagree.

## Not here

- **Helvetica (2).** A system font in Excalidraw too: `fonts/Helvetica/index.ts` registers
  `LOCAL_FONT_PROTOCOL`, which `fontFacesStylesGenerator` skips
  (`Fonts.ts@1118751f:301-304`). Nobody inlines it, least of all us.
- **Liberation Sans (9).** The file Excalidraw ships is Liberation 1.05, whose own ID 13
  points at the 1.x EULA (GPL v2 with a font exception), and that project's OFL covers 2.00
  and later only. Shipping 1.05 under the OFL would misstate its licence, and shipping 2.x
  would not be the oracle's bytes. `LICENSES.md:30` has the whole argument. **This is the
  one place our output is smaller than the oracle's** — the oracle inlines this family and
  we do not — and it is a licensing decision, not an oversight.
- **Xiaolai, Segoe UI Emoji.** CJK and emoji fallbacks, out of scope, 13MB and a system
  font respectively.

## Why a whole shard and not a subset

The oracle subsets each face to the codepoints the scene actually uses before inlining it
(`subset/subset-main.ts`, reached from `ExcalidrawFontFace.ts@1118751f:67`), which is why its
own committed snapshot carries ~1.5KB per declaration. We do not, and cannot without
harfbuzz — a WASM subsetter is a new dependency of a size this crate may not take
(BUNNY.md §3.2). So a declaration here is the whole shard, ~33% larger in base64 than the
raw bytes: one Virgil text costs 75KB of SVG where the oracle's costs about 2KB.

That is a size cost, not a behaviour one. The same glyphs are drawn at the same sizes, the
file is still self-contained, and `ponytail:` in `font_face.rs` names the upgrade path.
