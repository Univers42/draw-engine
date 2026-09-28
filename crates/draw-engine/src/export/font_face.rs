//! The `@font-face` declarations an exported SVG carries, and which families they are for.
//!
//! An SVG that names a font is a **claim addressed to a program that is not us**. Nothing
//! about a name is checked here — no browser ever loads this file — so a family spelled
//! `Lilita+One` renders as the viewer's fallback, silently, in every file that leaves the
//! building, while the app it was exported from looks perfect because the app *has* the
//! font. So the name, the family→file table and the rule for which families get a
//! declaration are all in this module and nowhere else. §2 does not allow the host to name a
//! font, and it is not asked to: the bytes are compiled in, so the export is synchronous,
//! takes no new argument and has no new failure. See "The host is not asked for anything".
//!
//! ## What the oracle does, and where
//!
//! - `exportToSvg` asks for the declarations, puts them in a `<style class="style-fonts">`
//!   inside the `<defs>`, and separates them with `"\n      "`
//!   (`packages/excalidraw/scene/export.ts@1118751f:435-451`).
//! - Each declaration is `` `@font-face { font-family: ${family}; src: url(${content}); }` ``
//!   (`fonts/ExcalidrawFontFace.ts@1118751f:37-51`), the family interpolated raw and the
//!   content a `data:` URL of the subsetted woff2.
//! - The families are the ones the **text elements** use, in first-appearance order
//!   (`fonts/Fonts.ts@1118751f:421-432`), and a family with no registered faces or a local
//!   one is skipped (`:291-304`).
//!
//! ## Where we are deliberately different, and why
//!
//! **A whole shard, not a subset.** The oracle subsets to the scene's codepoints through a
//! WASM harfbuzz build (`subset/subset-main.ts`); harfbuzz is a new dependency this crate
//! may not take (§3.2), so a declaration here is the entire file. Same glyphs, same sizes,
//! same rendering — a larger file. One Virgil text costs 75KB of base64 where the oracle's
//! costs about 2KB, and a scene using all six shipped families carries ~307KB, and
//! 230KB more in the WASM module itself. That is the price of the smallest thing that works
//! and it is recorded rather than hidden: the alternative is a `needs <package>` and no
//! fonts at all.
//!
//! `ponytail: no subsetting, the whole shard is embedded — upgrade path is an hb-subset
//! feature and a pass over the glyphs, which needs a dependency decision first.`
//!
//! **No `unicode-range`, exactly as the oracle.** `toCSS` writes three properties and no
//! descriptor, so a family we ship in two shards emits two bare `@font-face` rules of the
//! same family, and the browser unions their glyph coverage. That is the same situation the
//! oracle is in — its per-codepoint subsets are bare rules too — so this is not a second
//! font-matching rule invented here, it is the oracle's. The app's own `fonts.css` does use
//! `unicode-range`, and its per-family shard order is what [`SHIPPED_FONT_FACES`] copies so
//! the two agree on which shard is which.
//!
//! **Two families carry no declaration at all.** Helvetica is a system font in the oracle
//! too, and Liberation Sans 1.05's licence is unresolved — `assets/fonts/LICENSES.md`. The
//! `font-family` attribute on the `<text>` still names them, with their fallbacks, exactly
//! as it did before this existed; a viewer resolves those itself, which is the behaviour
//! `design.md:1348` had and the one the fallback list is for.
//!
//! ## The host is not asked for anything
//!
//! The first shape of this had the engine name a file, the host fetch it and hand the bytes
//! back — §2's network to the front, and the export format still in Rust. A test caught that
//! it bought nothing: the bytes are **compiled in**, so the round trip could only ever return
//! what the engine already had, and it introduced a real bug on the way (the compiled copy
//! shadowed the supplied one, so a host's bytes were silently ignored). The seam, the
//! second binding and the web module are gone. The engine has the font; an export needs the
//! font; there is nothing to fetch and nobody to ask.

use crate::scene::DrawElement;

/// One family this build can carry, with the shards it ships.
///
/// `files` is in the order `apps/web/src/lib/draw-chrome/fonts.css` declares the family —
/// Latin-Ext first for the four families that split that way, Latin first for Comic Shanns,
/// which is the order that app has, so a family means the same shards here as it does on
/// screen.
pub struct ShippedFace {
    /// The family's id — a key into [`crate::text::font::FAMILIES`].
    pub id: u8,
    /// The CSS name, which is the **first entry of that family's stack** and nothing else:
    /// `getFontFamilyString` returns `${fontFamilyString}` plus the oracle's fallbacks
    /// (`packages/common/src/utils.ts@1118751f:123-136`), so a name with a space is written
    /// unquoted and unescaped, and the `@font-face` uses the same spelling.
    pub name: &'static str,
    /// The shipped shards, as paths under the app's static root.
    pub files: &'static [&'static str],
}

/// Every family an exported SVG can carry, by id.
pub const SHIPPED_FONT_FACES: &[ShippedFace] = &[
    ShippedFace {
        id: 1,
        name: "Virgil",
        files: &["/fonts/Virgil/Virgil-Regular.woff2"],
    },
    ShippedFace {
        id: 3,
        name: "Cascadia",
        files: &["/fonts/Cascadia/CascadiaCode-Regular.woff2"],
    },
    ShippedFace {
        id: 5,
        name: "Excalifont",
        files: &[
            "/fonts/Excalifont/Excalifont-Regular-be310b9bcd4f1a43f571c46df7809174.woff2",
            "/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2",
        ],
    },
    ShippedFace {
        id: 6,
        name: "Nunito",
        files: &[
            "/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTo3j6zbXWjgevT5.woff2",
            "/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2",
        ],
    },
    ShippedFace {
        id: 7,
        name: "Lilita One",
        files: &[
            "/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYE98RXi4EwSsbg.woff2",
            "/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2",
        ],
    },
    ShippedFace {
        id: 8,
        name: "Comic Shanns",
        files: &[
            "/fonts/ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2",
            "/fonts/ComicShanns/ComicShanns-Regular-fcb0fc02dcbee4c9846b3e2508668039.woff2",
        ],
    },
];

/// The family a declaration for this id would name, when there is one.
pub fn shipped(id: u8) -> Option<&'static ShippedFace> {
    SHIPPED_FONT_FACES.iter().find(|face| face.id == id)
}

/// The bytes of one shipped file, compiled in.
///
/// The whole export format lives in Rust, so the format's own payload does too: a host that
/// supplied these would be a host that could put anything in a file that claims to be a
/// drawing. `assets/fonts/LICENSES.md` is where the bytes came from and what they may be
/// shipped under.
fn bytes_of(path: &str) -> Option<&'static [u8]> {
    Some(match path {
        "/fonts/Virgil/Virgil-Regular.woff2" => {
            include_bytes!("../../assets/fonts/Virgil/Virgil-Regular.woff2").as_slice()
        }
        "/fonts/Cascadia/CascadiaCode-Regular.woff2" => {
            include_bytes!("../../assets/fonts/Cascadia/CascadiaCode-Regular.woff2").as_slice()
        }
        "/fonts/Excalifont/Excalifont-Regular-be310b9bcd4f1a43f571c46df7809174.woff2" => {
            include_bytes!(
                "../../assets/fonts/Excalifont/Excalifont-Regular-be310b9bcd4f1a43f571c46df7809174.woff2"
            )
            .as_slice()
        }
        "/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2" => {
            include_bytes!(
                "../../assets/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2"
            )
            .as_slice()
        }
        "/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTo3j6zbXWjgevT5.woff2" => {
            include_bytes!(
                "../../assets/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTo3j6zbXWjgevT5.woff2"
            )
            .as_slice()
        }
        "/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2" => {
            include_bytes!(
                "../../assets/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2"
            )
            .as_slice()
        }
        "/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYE98RXi4EwSsbg.woff2" => {
            include_bytes!(
                "../../assets/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYE98RXi4EwSsbg.woff2"
            )
            .as_slice()
        }
        "/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2" => {
            include_bytes!("../../assets/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2")
                .as_slice()
        }
        "/fonts/ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2" => {
            include_bytes!(
                "../../assets/fonts/ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2"
            )
            .as_slice()
        }
        "/fonts/ComicShanns/ComicShanns-Regular-fcb0fc02dcbee4c9846b3e2508668039.woff2" => {
            include_bytes!(
                "../../assets/fonts/ComicShanns/ComicShanns-Regular-fcb0fc02dcbee4c9846b3e2508668039.woff2"
            )
            .as_slice()
        }
        _ => return None,
    })
}

/// The families these elements' text uses, in first-appearance order, each once.
///
/// The oracle's `getUniqueFamilies` is a `Set` filled while walking the elements
/// (`fonts/Fonts.ts@1118751f:421-432`): first appearance, not sorted, not z-order, and
/// **text elements only** — a shape that happens to carry a family id must not pull a face
/// into the file. An id with no family, or one this engine does not know, is not a family
/// and is left out, which is what the oracle's unregistered-family `continue`
/// (`Fonts.ts@1118751f:291-299`) amounts to.
fn families_used(elements: &[&DrawElement]) -> Vec<&'static ShippedFace> {
    let mut out: Vec<&'static ShippedFace> = Vec::new();
    for element in elements {
        if element.is_deleted || element.kind != crate::scene::DrawElementType::Text {
            continue;
        }
        let Some(face) = crate::scene::resolved_font_family_id(element).and_then(shipped) else {
            continue;
        };
        if !out.iter().any(|seen| seen.id == face.id) {
            out.push(face);
        }
    }
    out
}

/// The files an export of these elements carries, in the order the declarations go in.
///
/// What [`font_face_defs`] embeds, named so a caller can say what a document is carrying
/// without parsing it — and what a test asserts against the declarations themselves rather
/// than against a second copy of the rule.
pub fn font_face_files(elements: &[&DrawElement]) -> Vec<&'static str> {
    families_used(elements)
        .into_iter()
        .flat_map(|face| face.files.iter().copied())
        .collect()
}

/// One declaration: the oracle's own text, with the family and a `data:` URL in it.
fn declaration(family: &str, payload: &str) -> String {
    format!("@font-face {{ font-family: {family}; src: url({payload}); }}")
}

/// The `<defs>` an SVG of these elements carries its font declarations in.
///
/// Always written, even when it is empty, because the oracle writes it unconditionally
/// (`export.ts@1118751f:445-451`): a shapes-only drawing has a `<defs>` holding an empty
/// `<style>`, and giving it no `<defs>` at all would be a second shape of document.
///
/// Every family here is one the elements use and we ship a file for. There is no argument
/// for a caller to widen it with: a drawing is exported with the faces it needs, and the
/// only inputs are the elements.
pub fn font_face_defs(elements: &[&DrawElement]) -> String {
    let declarations: Vec<String> = families_used(elements)
        .into_iter()
        .flat_map(|face| face.files.iter().map(|path| (face.name, *path)))
        .filter_map(|(family, path)| {
            bytes_of(path).map(|bytes| declaration(family, &data_url(bytes)))
        })
        .collect();
    format!(
        "<defs><style class=\"style-fonts\">{delimiter}{body}</style></defs>",
        // The oracle's own separator, and it is character data: `const delimiter = "\n      ";`
        // (6 spaces) at `export.ts@1118751f:443`.
        delimiter = DELIMITER,
        body = declarations.join(DELIMITER),
    )
}

/// The separator between declarations, and the one before the first.
///
/// `const delimiter = "\n      "; // 6 spaces` (`export.ts@1118751f:443`).
const DELIMITER: &str = "\n      ";

/// `data:font/woff2;base64,…` — the form `ExcalidrawFontFace.getContent` returns
/// (`ExcalidrawFontFace.ts@1118751f:67`), which is what `subsetWoff2GlyphsByCodepoints`
/// produces and what therefore goes in the file.
///
/// Base64 is ASCII with no `<`, no `&` and no `--`, so the style's character data needs no
/// escaping and needs no CDATA — and that is a property of the alphabet, not a hope: a
/// payload that could contain `]]>` would be a document this module could emit broken.
fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:font/woff2;base64,{}",
        crate::export::base64::encode(bytes)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::create_element_default;
    use crate::scene::DrawElementType;
    use crate::scene::Geometry;

    fn borrowed_of(elements: &[DrawElement]) -> Vec<&DrawElement> {
        elements.iter().collect()
    }

    fn text(family: Option<u8>) -> DrawElement {
        let mut element = create_element_default(
            DrawElementType::Text,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        );
        element.text = Some("x".into());
        element.font_family = family;
        element
    }

    #[test]
    fn every_shipped_name_is_the_first_entry_of_that_familys_stack() {
        for face in SHIPPED_FONT_FACES {
            let stack = crate::text::font::css_stack(Some(face.id));
            assert_eq!(
                stack.split(", ").next(),
                Some(face.name),
                "family {}",
                face.id
            );
        }
    }

    #[test]
    fn every_shipped_file_is_compiled_in_and_is_a_woff2() {
        for face in SHIPPED_FONT_FACES {
            for path in face.files {
                let bytes = bytes_of(path).unwrap_or_else(|| panic!("{path}"));
                assert_eq!(&bytes[..4], b"wOF2", "{path}");
            }
        }
        assert_eq!(bytes_of("/fonts/Nope.woff2"), None);
    }

    #[test]
    fn the_families_are_the_texts_in_the_order_they_appear() {
        let scene: Vec<DrawElement> = vec![text(Some(6)), text(Some(1)), text(Some(6))];
        let names: Vec<&str> = families_used(&borrowed_of(&scene))
            .into_iter()
            .map(|face| face.name)
            .collect();
        assert_eq!(
            names,
            ["Nunito", "Virgil"],
            "one face per family, first appearance"
        );
        // A shape and a tombstone pull nothing in, which is the near-miss a corpus without
        // them would miss.
        let mut shape = crate::scene::create_element_default(
            DrawElementType::Rectangle,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        );
        shape.font_family = Some(5);
        let mut gone = text(Some(8));
        gone.is_deleted = true;
        let mixed: Vec<DrawElement> = vec![shape, gone];
        assert!(families_used(&borrowed_of(&mixed)).is_empty());
    }

    #[test]
    fn no_family_is_shipped_twice() {
        for (index, face) in SHIPPED_FONT_FACES.iter().enumerate() {
            let rest = &SHIPPED_FONT_FACES[index + 1..];
            assert!(rest.iter().all(|other| other.id != face.id));
            assert!(rest.iter().all(|other| other.name != face.name));
        }
    }
}
