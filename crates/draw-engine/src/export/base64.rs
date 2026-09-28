//! Base64, the standard alphabet with padding (RFC 4648 §4).
//!
//! Written out rather than pulled in: it is 20 lines, and every crate that does it — the
//! `base64` family — is a dependency BUNNY.md §3.2 does not allow, for a codec this crate
//! now has two callers of. The *reader* stays with the module that owns it
//! ([`crate::export::roundtrip`]), because that is the only thing that reads a payload; this
//! is the writer, and two things write one.

/// The 64 characters, in the order RFC 4648 §4 numbers them. `pub` because the scene
/// payload's reader is strict about which characters it accepts and has to agree with the
/// writer about what the alphabet is.
pub const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The bytes as base64, padded to a multiple of four.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        // Left-aligned into 24 bits before the shifts, so the last group — which is one or
        // two bytes, not three — lands in the same place as every other one. Without it a
        // two-byte tail is read one whole byte too far left and the file comes back with a
        // character flipped in it; see the RFC 4648 vectors in
        // `ci_export_roundtrip_bytes.rs`, which is where that is pinned.
        let packed = group
            .iter()
            .fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte))
            << (8 * (3 - group.len()));
        for index in 0..=group.len() {
            out.push(ALPHABET[((packed >> (18 - 6 * index)) & 0x3F) as usize] as char);
        }
        for _ in group.len()..3 {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648 §10's test vectors, which are the codec's own oracle (a document, not a
    /// table of one's own devising).
    #[test]
    fn the_rfc_4648_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), encoded, "{plain:?}");
        }
    }

    #[test]
    fn every_tail_length_lands_where_the_body_ends() {
        // 60 bytes is a whole number of three-byte groups, so the three tails below land on
        // the three shapes the last group can take — which is where this codec's one real
        // failure mode is, and a two-byte tail is the one that is invisible in a hex dump.
        let long = vec![0xABu8; 60];
        for tail in 0..3usize {
            let mut bytes = long.clone();
            bytes.extend((0..tail).map(|i| i as u8));
            let encoded = encode(&bytes);
            assert_eq!(encoded.len() % 4, 0, "{tail}");
            // Zero, one or two `=` by how many bytes the last group is short of three, and
            // **never three** — which is what counting the tail twice would produce.
            let expected = (3 - bytes.len() % 3) % 3;
            assert_eq!(encoded.matches('=').count(), expected, "{tail}");
        }
    }
}
