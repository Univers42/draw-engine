//! Turning a link someone pasted into something that can be embedded.
//!
//! All of this is string work and none of it is rendering, so it lives in the motor.
//! Two frontends given the same YouTube link must produce the same embed URL and the
//! same shape, or the same board looks different depending on who opened it.
//!
//! Transcribed from Excalidraw's `element/embeddable.ts` at the SHA pinned in
//! `scripts/oracle-sha.txt`. Three things it does, in order:
//!
//! 1. decide whether the host is one we will embed at all;
//! 2. rewrite the page URL into the provider's *embed* URL, because pasting a YouTube
//!    watch page into an iframe shows a refusal, not a video;
//! 3. hand back the intrinsic shape, because an embed that arrives square and has to be
//!    reshaped by hand is worse than one that arrives the right way up.
//!
//! Parsed structurally rather than with regular expressions. The regexes in the original
//! are the part most likely to be subtly wrong — `youtu.be` inside a path, a host that
//! merely *ends* with `figma.com` — and splitting the URL into host, path and query
//! makes those questions explicit instead of incidental.
//!
//! Deferred, and listed so their absence is a decision rather than an oversight: GitHub
//! gists, Twitter/X, Reddit, Giphy, val.town, Microsoft Forms and the `<iframe>`/
//! `<blockquote>` snippet forms. Each is another rewrite in the same shape as these.

/// What an embed turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedKind {
    /// Something that plays. Worth knowing because a video wants a 16:9 box.
    Video,
    Generic,
}

/// A link resolved into something embeddable.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbedLink {
    /// The URL to put in the frame — usually *not* the URL that was pasted.
    pub url: String,
    /// The shape this provider wants, as a width and height in pixels.
    pub intrinsic_width: f64,
    pub intrinsic_height: f64,
    pub kind: EmbedKind,
    /// Whether the frame may keep its origin.
    ///
    /// Off by default, which is the safe direction: a same-origin frame can reach the
    /// embedding page's storage and cookies. It is granted only to the providers whose
    /// players do not work without it, which is the list Excalidraw arrived at.
    pub allow_same_origin: bool,
}

/// The shape an embed gets when the provider has no opinion. Excalidraw's default.
pub const DEFAULT_EMBED_WIDTH: f64 = 560.0;
pub const DEFAULT_EMBED_HEIGHT: f64 = 840.0;
/// A landscape video.
pub const VIDEO_EMBED_WIDTH: f64 = 560.0;
pub const VIDEO_EMBED_HEIGHT: f64 = 315.0;

/// Hosts that may be embedded at all.
///
/// An allow-list, not a block-list. An iframe pointing at an arbitrary URL is a hole in
/// the page: it can cover the board, and — same-origin or not — it is content someone
/// else controls rendering inside ours. Excalidraw keeps this list for the same reason.
pub const ALLOWED_EMBED_HOSTS: [&str; 16] = [
    "youtube.com",
    "youtu.be",
    "vimeo.com",
    "player.vimeo.com",
    "drive.google.com",
    "figma.com",
    "link.excalidraw.com",
    "gist.github.com",
    "twitter.com",
    "x.com",
    "simplepdf.eu",
    "stackblitz.com",
    "val.town",
    "giphy.com",
    "reddit.com",
    "forms.microsoft.com",
];

/// Hosts whose players do not work in a frame stripped of its origin.
pub const SAME_ORIGIN_EMBED_HOSTS: [&str; 11] = [
    "youtube.com",
    "youtu.be",
    "vimeo.com",
    "player.vimeo.com",
    "drive.google.com",
    "figma.com",
    "twitter.com",
    "x.com",
    "simplepdf.eu",
    "stackblitz.com",
    "reddit.com",
];

/// A URL split into the parts the rules actually ask about.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Parts<'a> {
    host: &'a str,
    path: &'a str,
    query: &'a str,
}

/// Split a URL, tolerating a missing scheme and a leading `www.`.
///
/// People paste what they copied, and what they copied is often `youtu.be/abc` with no
/// scheme at all. Refusing those would make the feature feel broken for the most common
/// way of arriving at it.
fn split_url(raw: &str) -> Option<Parts<'_>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // A fragment never reaches the server, so it never affects which embed this is.
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);

    let after_scheme = match without_fragment.find("://") {
        Some(index) => {
            let scheme = &without_fragment[..index];
            // Only the web schemes. `javascript:` and `data:` in a frame are exactly the
            // holes the allow-list exists to close.
            if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
                return None;
            }
            &without_fragment[index + 3..]
        }
        None => {
            // No scheme at all is fine, but something that looks like `mailto:x` or
            // `javascript:alert(1)` is not a bare host.
            if let Some(colon) = without_fragment.find(':') {
                let before = &without_fragment[..colon];
                if !before.contains('/') && !before.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
            }
            without_fragment
        }
    };

    let (authority, rest) = match after_scheme.find('/') {
        Some(index) => (&after_scheme[..index], &after_scheme[index..]),
        None => (after_scheme, ""),
    };
    // Credentials in an embed URL are a mistake at best, so they are not carried through.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let host = authority.split(':').next().unwrap_or(authority);
    let host = host.trim_start_matches("www.");
    if host.is_empty() {
        return None;
    }

    let (path, query) = match rest.find('?') {
        Some(index) => (&rest[..index], &rest[index + 1..]),
        None => (rest, ""),
    };

    Some(Parts { host, path, query })
}

/// Whether the host is on the allow-list, matching a bare domain or any subdomain of it.
///
/// Subdomains count so that `player.vimeo.com` is reached by `vimeo.com`, and so that
/// the `*.simplepdf.eu` entry means what it looks like. Matched on a dot boundary, so
/// `notyoutube.com` does not slip past `youtube.com`.
pub fn is_allowed_embed_host(host: &str) -> bool {
    matches_any(host, &ALLOWED_EMBED_HOSTS)
}

fn matches_any(host: &str, list: &[&str]) -> bool {
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    list.iter().any(|allowed| {
        let allowed = allowed.trim_start_matches("*.");
        host == allowed || host.ends_with(&format!(".{allowed}"))
    })
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

/// Seconds into a video, from a `t` or `start` parameter.
///
/// Accepts both a plain count and YouTube's `1h2m3s`, because both are produced by their
/// own share dialog depending on where you copy from.
pub fn parse_timestamp(query: &str) -> u64 {
    let Some(raw) = query_value(query, "t").or_else(|| query_value(query, "start")) else {
        return 0;
    };
    if let Ok(seconds) = raw.parse::<u64>() {
        return seconds;
    }

    let mut total = 0u64;
    let mut current = 0u64;
    let mut saw_unit = false;
    for ch in raw.chars() {
        match ch {
            '0'..='9' => current = current.saturating_mul(10) + (ch as u64 - '0' as u64),
            'h' => {
                total = total.saturating_add(current.saturating_mul(3600));
                current = 0;
                saw_unit = true;
            }
            'm' => {
                total = total.saturating_add(current.saturating_mul(60));
                current = 0;
                saw_unit = true;
            }
            's' => {
                total = total.saturating_add(current);
                current = 0;
                saw_unit = true;
            }
            // Anything else means this is not a timestamp we understand, and guessing
            // would start the video somewhere arbitrary.
            _ => return 0,
        }
    }
    if saw_unit {
        total
    } else {
        0
    }
}

/// A YouTube video or playlist id, and whether it is a portrait short.
fn youtube_target(parts: Parts<'_>) -> Option<(String, bool)> {
    let segments: Vec<&str> = parts.path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.host == "youtu.be" {
        return segments.first().map(|id| (format!("embed/{id}"), false));
    }

    match segments.as_slice() {
        ["watch"] => query_value(parts.query, "v").map(|id| (format!("embed/{id}"), false)),
        ["shorts", id] => Some((format!("embed/{id}"), true)),
        ["embed", "videoseries"] => query_value(parts.query, "list")
            .map(|list| (format!("embed/videoseries?list={list}"), false)),
        ["embed", id] => Some((format!("embed/{id}"), false)),
        ["playlist"] => query_value(parts.query, "list")
            .map(|list| (format!("embed/videoseries?list={list}"), false)),
        _ => None,
    }
}

/// Percent-encode a URL so it can be carried inside another URL's query string.
///
/// Only the unreserved set survives. Anything looser lets a `&` in the inner URL end the
/// parameter early, and the embed silently points somewhere else.
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Resolve a pasted link into something embeddable, or nothing.
///
/// Returning `None` covers both "we do not embed that host" and "that is not a URL". The
/// caller has nothing useful to do differently between the two, and a host may not be
/// embeddable today and be tomorrow.
pub fn embed_link(raw: &str) -> Option<EmbedLink> {
    let parts = split_url(raw)?;
    if !is_allowed_embed_host(parts.host) {
        return None;
    }
    let allow_same_origin = matches_any(parts.host, &SAME_ORIGIN_EMBED_HOSTS);

    let video = |url: String, portrait: bool| EmbedLink {
        url,
        intrinsic_width: if portrait {
            VIDEO_EMBED_HEIGHT
        } else {
            VIDEO_EMBED_WIDTH
        },
        intrinsic_height: if portrait {
            VIDEO_EMBED_WIDTH
        } else {
            VIDEO_EMBED_HEIGHT
        },
        kind: EmbedKind::Video,
        allow_same_origin,
    };

    if parts.host == "youtube.com" || parts.host == "youtu.be" {
        let (target, portrait) = youtube_target(parts)?;
        let seconds = parse_timestamp(parts.query);
        let joiner = if target.contains('?') { '&' } else { '?' };
        let start = if seconds > 0 {
            format!("&start={seconds}")
        } else {
            String::new()
        };
        return Some(video(
            format!("https://www.youtube.com/{target}{joiner}enablejsapi=1{start}"),
            portrait,
        ));
    }

    if parts.host == "vimeo.com" || parts.host == "player.vimeo.com" {
        let id = parts.path.rsplit('/').find(|s| !s.is_empty())?;
        // Their player takes a numeric id and nothing else; a slug would load a page
        // that refuses to frame, which looks like our bug rather than a bad link.
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        return Some(video(
            format!("https://player.vimeo.com/video/{id}?api=1"),
            false,
        ));
    }

    if parts.host == "drive.google.com" {
        let segments: Vec<&str> = parts.path.split('/').filter(|s| !s.is_empty()).collect();
        let file_id = match segments.as_slice() {
            ["file", "d", id, ..] => Some(*id),
            ["open"] | ["uc"] => query_value(parts.query, "id"),
            _ => None,
        }?;
        if !file_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        let mut search = Vec::new();
        if let Some(key) = query_value(parts.query, "resourcekey") {
            search.push(format!("resourcekey={key}"));
        }
        let seconds = parse_timestamp(parts.query);
        if seconds > 0 {
            search.push(format!("t={seconds}"));
        }
        let tail = if search.is_empty() {
            String::new()
        } else {
            format!("?{}", search.join("&"))
        };
        return Some(video(
            format!("https://drive.google.com/file/d/{file_id}/preview{tail}"),
            false,
        ));
    }

    if parts.host == "figma.com" {
        // Figma is given the *original* link, encoded, because their embed endpoint
        // takes the file URL as a parameter rather than being a different path.
        let canonical = format!(
            "https://www.figma.com{}{}",
            parts.path,
            if parts.query.is_empty() {
                String::new()
            } else {
                format!("?{}", parts.query)
            }
        );
        return Some(EmbedLink {
            url: format!(
                "https://www.figma.com/embed?embed_host=share&url={}",
                encode_component(&canonical)
            ),
            intrinsic_width: 550.0,
            intrinsic_height: 550.0,
            kind: EmbedKind::Generic,
            allow_same_origin,
        });
    }

    // An allowed host with no special handling is framed as it stands.
    Some(EmbedLink {
        url: format!(
            "https://{}{}{}",
            parts.host,
            parts.path,
            if parts.query.is_empty() {
                String::new()
            } else {
                format!("?{}", parts.query)
            }
        ),
        intrinsic_width: DEFAULT_EMBED_WIDTH,
        intrinsic_height: DEFAULT_EMBED_HEIGHT,
        kind: EmbedKind::Generic,
        allow_same_origin,
    })
}
