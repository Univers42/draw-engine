pub mod base64;
pub mod clipboard;
pub mod font_face;
pub mod json;
pub mod png;
pub mod png_chunk;
pub mod roundtrip;
pub mod scope;
pub mod svg;

pub use clipboard::*;
pub use font_face::*;
pub use json::*;
pub use png::*;
pub use png_chunk::*;
pub use roundtrip::*;
pub use scope::*;
pub use svg::*;

/// Why a byte sequence is not a PNG we can walk.
///
/// Four, and the oracle's own answer is none of them: `png-chunks-extract` throws, and
/// `decodePngMetadata` lets the throw out because `getTEXtChunk` is awaited outside its
/// `try` (`data/image.ts@1118751f:50`). Splitting them is what lets
/// [`crate::RestoreRefusal`] say "that is not a picture" rather than name a library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PngError {
    /// No PNG signature. Not an image.
    NotAPng,
    /// A chunk claims more bytes than the file has. A damaged file, or not one.
    Truncated,
    /// The last chunk is not `IEND`, so there is no "before the last chunk" to insert at.
    NoEnd,
    /// A keyword that is empty, 80 bytes long, or holds a NUL — `png-chunk-text`'s three
    /// checks (`encode.js:7-13, 20-24`).
    BadKeyword,
    /// A text with a character above U+00FF or a NUL in it, which a Latin-1 `tEXt` reader
    /// cannot give back unchanged (`encode.js:7-8`, `decode.js:25`).
    NotLatin1,
}
