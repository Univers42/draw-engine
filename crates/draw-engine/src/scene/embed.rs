//! Turning a link someone pasted into something that can be embedded.
//!
//! All of this is string work and none of it is rendering, so it lives in the motor.
//! Two frontends given the same YouTube link must produce the same embed URL and the
//! same shape, or the same board looks different depending on who opened it.
//!
//! Started as a transcription of Excalidraw's `element/embeddable.ts` at the SHA pinned
//! in `scripts/oracle-sha.txt`, and extended past it. Three things it does, in order:
//!
//! 1. decide whether the link is one we will embed at all;
//! 2. rewrite the page URL into the provider's *embed* URL, because pasting a YouTube
//!    watch page into an iframe shows a refusal, not a video — and most of the web
//!    refuses to be framed at its own address;
//! 3. hand back the intrinsic shape, because an embed that arrives square and has to be
//!    reshaped by hand is worse than one that arrives the right way up.
//!
//! Parsed structurally rather than with regular expressions. The regexes in the original
//! are the part most likely to be subtly wrong — `youtu.be` inside a path, a host that
//! merely *ends* with `figma.com` — and splitting the URL into host, path and query
//! makes those questions explicit instead of incidental.
//!
//! # A provider's page is not an embed
//!
//! Every provider here has a page you share and a different address its player lives
//! at, and only the second may be framed: the first sends `X-Frame-Options` or a
//! `frame-ancestors` policy and the frame shows a refusal. So a provider with a rewrite
//! refuses a link it does not recognise, rather than framing it as it stands — a channel
//! page or a profile would arrive as an empty box that looks like our bug. Only the
//! providers whose ordinary pages do frame (StackBlitz, val.town, Excalidraw, SimplePDF)
//! fall through to the link as given.
//!
//! # Idempotent
//!
//! What this returns resolves to itself. The board stores the resolved URL, and the frame
//! is resolved again from it each time it is shown — which is what lets a board saved
//! under older rules pick up a fix to them — so a rewrite that did not survive a second
//! pass would wrap a Figma embed in another Figma embed every time a board was opened.

/// What an embed turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedKind {
    /// Something that plays. Worth knowing because a video wants a 16:9 box, and because
    /// the host keeps a player's viewport legible when the board is zoomed out.
    Video,
    Generic,
}

/// A link resolved into something embeddable.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbedLink {
    /// The URL to put in the frame — usually *not* the URL that was pasted. For a
    /// [`document`](Self::document) embed it is the canonical page, kept so the board
    /// stores something that resolves again.
    pub url: String,
    /// The shape this provider wants, as a width and height in pixels.
    pub intrinsic_width: f64,
    pub intrinsic_height: f64,
    pub kind: EmbedKind,
    /// Whether the frame may keep its origin.
    ///
    /// A frame loaded from the provider's own address keeps the *provider's* origin,
    /// never ours, so granting it lets their player use its own storage and nothing
    /// more — and most players do not work without it. A [`document`](Self::document) is
    /// different: its origin would be the board's own, and a script in it could reach
    /// into the page. It is never granted there.
    pub allow_same_origin: bool,
    /// The HTML to frame instead of a URL, for a provider with no address that can be
    /// framed at all — a gist is only offered as a script that writes itself into the
    /// page that loads it.
    pub document: Option<String>,
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
/// the page: it can cover the board, and it is content someone else controls rendering
/// inside ours. Excalidraw keeps this list for the same reason. Being on it is necessary,
/// not sufficient: each provider also has to recognise the link — see the module notes.
pub const ALLOWED_EMBED_HOSTS: [&str; 44] = [
    "youtube.com",
    "youtu.be",
    "youtube-nocookie.com",
    "vimeo.com",
    "player.vimeo.com",
    "drive.google.com",
    "docs.google.com",
    "calendar.google.com",
    "google.com",
    "figma.com",
    "link.excalidraw.com",
    "excalidraw.com",
    "gist.github.com",
    "twitter.com",
    "x.com",
    "simplepdf.eu",
    "stackblitz.com",
    "val.town",
    "giphy.com",
    "reddit.com",
    "forms.microsoft.com",
    "forms.office.com",
    "loom.com",
    "open.spotify.com",
    "soundcloud.com",
    "dailymotion.com",
    "dai.ly",
    "tiktok.com",
    "streamable.com",
    "twitch.tv",
    "facebook.com",
    "fb.watch",
    "instagram.com",
    "bilibili.com",
    "codepen.io",
    "codesandbox.io",
    "jsfiddle.net",
    "openstreetmap.org",
    "podcasts.apple.com",
    "music.apple.com",
    "mixcloud.com",
    "ted.com",
    "desmos.com",
    "miro.com",
];

/// A URL split into the parts the rules actually ask about.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Parts<'a> {
    /// Without `www.` or a mobile prefix: what the rules match on.
    host: &'a str,
    /// As it was written, for a link that is framed as it stands.
    written_host: &'a str,
    path: &'a str,
    query: &'a str,
}

impl<'a> Parts<'a> {
    fn segments(&self) -> Vec<&'a str> {
        self.path.split('/').filter(|s| !s.is_empty()).collect()
    }

    fn query(&self, key: &str) -> Option<&'a str> {
        query_value(self.query, key)
    }

    /// `https://host/path?query`, as given.
    fn as_given(&self) -> String {
        let mut url = format!("https://{}{}", self.written_host, self.path);
        if !self.query.is_empty() {
            url.push('?');
            url.push_str(self.query);
        }
        url
    }
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

    let (authority, rest) = match after_scheme.find(['/', '?']) {
        Some(index) => (&after_scheme[..index], &after_scheme[index..]),
        None => (after_scheme, ""),
    };
    // Credentials in an embed URL are a mistake at best, so they are not carried through.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let written_host = authority.split(':').next().unwrap_or(authority);
    // `www.` and the mobile site are the same provider as the bare domain.
    let host = written_host
        .trim_start_matches("www.")
        .trim_start_matches("m.")
        .trim_start_matches("mobile.");
    if host.is_empty() {
        return None;
    }

    let (path, query) = match rest.find('?') {
        Some(index) => (&rest[..index], &rest[index + 1..]),
        None => (rest, ""),
    };

    Some(Parts {
        host,
        written_host,
        path,
        query,
    })
}

/// Whether the host is on the allow-list, matching a bare domain or any subdomain of it.
///
/// Subdomains count so that `player.vimeo.com` is reached by `vimeo.com`, and so that
/// the `*.simplepdf.eu` entry means what it looks like. Matched on a dot boundary, so
/// `notyoutube.com` does not slip past `youtube.com`.
pub fn is_allowed_embed_host(host: &str) -> bool {
    provider_of(host).is_some()
}

/// Which allow-listed domain a host belongs to, if any.
fn provider_of(host: &str) -> Option<&'static str> {
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    ALLOWED_EMBED_HOSTS.iter().copied().find(|allowed| {
        host == *allowed
            || host
                .strip_suffix(allowed)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

/// Whether `value` is safe to put into a URL we build: an id, not a path or a query.
///
/// Every id taken from a pasted link goes straight into a URL on someone else's domain.
/// Anything outside this set — a `/`, a `?`, a `%2F` — is a chance to build a URL that
/// points somewhere the rules never looked at.
fn is_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// As [`is_id`], also allowing the dots of a dotted name (`user.val`, `@user`).
fn is_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'@'))
        && !value.contains("..")
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

/// The link someone copied out of an embed snippet, or the input unchanged.
///
/// Providers offer an `<iframe>` or a `<blockquote>` to paste into a web page, and that
/// is often what people copy. Excalidraw's `maybeParseEmbedSrc`: the URL is taken out of
/// the snippet and resolved like any other link, so a snippet is never trusted further
/// than the link inside it.
pub fn embed_src_of(raw: &str) -> &str {
    let trimmed = raw.trim();
    if !trimmed.starts_with('<') {
        return trimmed;
    }
    let attribute = |name: &str| -> Option<&str> {
        let lower = trimmed.to_ascii_lowercase();
        let mut from = 0;
        while let Some(found) = lower[from..].find(name) {
            let start = from + found + name.len();
            let quote = trimmed[start..].chars().next()?;
            if quote == '"' || quote == '\'' {
                let value = &trimmed[start + 1..];
                let end = value.find(quote)?;
                return Some(&value[..end]);
            }
            from = start;
        }
        None
    };
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("<blockquote") {
        // A tweet or a Reddit post: the link to it is the first `href`.
        return attribute(" href=").unwrap_or(trimmed);
    }
    if lower.starts_with("<script") {
        // A gist is offered as `<script src="https://gist.github.com/u/id.js">`.
        return attribute(" src=")
            .map(|src| src.strip_suffix(".js").unwrap_or(src))
            .unwrap_or(trimmed);
    }
    attribute(" src=").unwrap_or(trimmed)
}

/// Resolve a pasted link into something embeddable, or nothing.
///
/// Returning `None` covers both "we do not embed that host" and "that is not a URL". The
/// caller has nothing useful to do differently between the two, and a host may not be
/// embeddable today and be tomorrow.
pub fn embed_link(raw: &str) -> Option<EmbedLink> {
    let parts = split_url(embed_src_of(raw))?;
    let provider = provider_of(parts.host)?;
    let mut link = resolve(provider, parts)?;
    link.allow_same_origin = link.document.is_none();
    Some(link)
}

fn sized(url: String, kind: EmbedKind, width: f64, height: f64) -> EmbedLink {
    EmbedLink {
        url,
        intrinsic_width: width,
        intrinsic_height: height,
        kind,
        allow_same_origin: true,
        document: None,
    }
}

fn video(url: String) -> EmbedLink {
    sized(url, EmbedKind::Video, VIDEO_EMBED_WIDTH, VIDEO_EMBED_HEIGHT)
}

fn portrait_video(url: String) -> EmbedLink {
    sized(url, EmbedKind::Video, VIDEO_EMBED_HEIGHT, VIDEO_EMBED_WIDTH)
}

fn page(url: String, width: f64, height: f64) -> EmbedLink {
    sized(url, EmbedKind::Generic, width, height)
}

fn resolve(provider: &str, parts: Parts<'_>) -> Option<EmbedLink> {
    match provider {
        "youtube.com" | "youtu.be" | "youtube-nocookie.com" => youtube(parts),
        "vimeo.com" | "player.vimeo.com" => vimeo(parts),
        "drive.google.com" => google_drive(parts),
        "docs.google.com" => google_docs(parts),
        "calendar.google.com" => parts
            .path
            .contains("/embed")
            .then(|| page(parts.as_given(), 800.0, 600.0)),
        // All of Google on one domain: only the maps embed endpoint, never the rest.
        "google.com" => (matches!(parts.host, "google.com" | "maps.google.com")
            && parts.path.starts_with("/maps/embed"))
        .then(|| page(parts.as_given(), 600.0, 450.0)),
        "figma.com" => figma(parts),
        "gist.github.com" => gist(parts),
        "twitter.com" | "x.com" => tweet(parts),
        "reddit.com" => reddit(parts),
        "giphy.com" => giphy(parts),
        "val.town" => Some(val_town(parts)),
        "forms.microsoft.com" | "forms.office.com" => {
            let mut url = parts.as_given();
            if query_value(parts.query, "embed") != Some("true") {
                url.push(if parts.query.is_empty() { '?' } else { '&' });
                url.push_str("embed=true");
            }
            Some(page(url, DEFAULT_EMBED_WIDTH, DEFAULT_EMBED_HEIGHT))
        }
        "stackblitz.com" => {
            let mut url = parts.as_given();
            if query_value(parts.query, "embed").is_none() {
                url.push(if parts.query.is_empty() { '?' } else { '&' });
                url.push_str("embed=1");
            }
            Some(page(url, 800.0, 560.0))
        }
        "simplepdf.eu" | "link.excalidraw.com" | "excalidraw.com" => Some(page(
            parts.as_given(),
            DEFAULT_EMBED_WIDTH,
            DEFAULT_EMBED_HEIGHT,
        )),
        "loom.com" => loom(parts),
        "open.spotify.com" => spotify(parts),
        "soundcloud.com" => soundcloud(parts),
        "dailymotion.com" | "dai.ly" => dailymotion(parts),
        "tiktok.com" => tiktok(parts),
        "streamable.com" => streamable(parts),
        "twitch.tv" => twitch(parts),
        "facebook.com" | "fb.watch" => facebook(parts),
        "instagram.com" => instagram(parts),
        "bilibili.com" => bilibili(parts),
        "codepen.io" => codepen(parts),
        "codesandbox.io" => codesandbox(parts),
        "jsfiddle.net" => jsfiddle(parts),
        "openstreetmap.org" => {
            (parts.path == "/export/embed.html").then(|| page(parts.as_given(), 600.0, 450.0))
        }
        "podcasts.apple.com" | "music.apple.com" => apple(provider, parts),
        "mixcloud.com" => mixcloud(parts),
        "ted.com" => ted(parts),
        "desmos.com" => desmos(parts),
        "miro.com" => miro(parts),
        _ => None,
    }
}

// ----------------------------------------------------------------------- video

fn youtube(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let id = |id: &str| is_id(id).then(|| format!("embed/{id}"));
    let list = |list: &str| is_id(list).then(|| format!("embed/videoseries?list={list}"));

    let (target, portrait) = if parts.host == "youtu.be" {
        (id(segments.first()?)?, false)
    } else {
        match segments.as_slice() {
            ["watch"] => (id(parts.query("v")?)?, false),
            ["shorts", v] => (id(v)?, true),
            ["live" | "v" | "e", v] => (id(v)?, false),
            ["embed", "videoseries"] => (list(parts.query("list")?)?, false),
            ["embed", v] => (id(v)?, false),
            ["playlist"] => (list(parts.query("list")?)?, false),
            _ => return None,
        }
    };

    let seconds = parse_timestamp(parts.query);
    let joiner = if target.contains('?') { '&' } else { '?' };
    let start = if seconds > 0 {
        format!("&start={seconds}")
    } else {
        String::new()
    };
    let url = format!("https://www.youtube.com/{target}{joiner}enablejsapi=1{start}");
    Some(if portrait {
        portrait_video(url)
    } else {
        video(url)
    })
}

fn vimeo(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    // Their player takes a numeric id and nothing else; a slug would load a page that
    // refuses to frame, which looks like our bug rather than a bad link. The id is the
    // first number on the path — after `channels/x/` or `video/` or nothing — and an
    // unlisted video carries its privacy hash either as the next segment or as `h=`.
    let at = segments
        .iter()
        .position(|s| s.bytes().all(|b| b.is_ascii_digit()))?;
    let id = segments[at];
    let hash = parts
        .query("h")
        .or_else(|| segments.get(at + 1).copied())
        .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()) && !h.is_empty());
    let mut url = format!("https://player.vimeo.com/video/{id}?api=1");
    if let Some(hash) = hash {
        url.push_str("&h=");
        url.push_str(hash);
    }
    Some(video(url))
}

fn dailymotion(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let raw = if parts.host == "dai.ly" {
        *segments.first()?
    } else {
        match segments.as_slice() {
            ["video", id] | ["embed", "video", id] => id,
            ["player.html"] => parts.query("video")?,
            _ => return None,
        }
    };
    // `/video/x8abcd_some-title` is a common shape of the same link.
    let id = raw.split('_').next()?;
    is_id(id).then(|| video(format!("https://www.dailymotion.com/embed/video/{id}")))
}

fn tiktok(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let id = match segments.as_slice() {
        [user, "video", id] if user.starts_with('@') => *id,
        ["embed", "v2", id] | ["embed", id] => *id,
        _ => return None,
    };
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then(|| {
        sized(
            format!("https://www.tiktok.com/embed/v2/{id}"),
            EmbedKind::Video,
            340.0,
            700.0,
        )
    })
}

fn streamable(parts: Parts<'_>) -> Option<EmbedLink> {
    let id = match parts.segments().as_slice() {
        [id] | ["e" | "o" | "s", id] => *id,
        _ => return None,
    };
    is_id(id).then(|| video(format!("https://streamable.com/e/{id}")))
}

/// Twitch refuses to play unless told which site is embedding it, as `parent=`, and only
/// the host knows its own hostname — so it adds that. The link here carries none.
fn twitch(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    if parts.host == "player.twitch.tv" {
        if let Some(channel) = parts.query("channel").filter(|c| is_id(c)) {
            return Some(video(format!(
                "https://player.twitch.tv/?channel={channel}"
            )));
        }
        let id = parts.query("video").filter(|v| is_id(v))?;
        return Some(video(format!("https://player.twitch.tv/?video={id}")));
    }
    if parts.host == "clips.twitch.tv" {
        let slug = match segments.as_slice() {
            ["embed"] => parts.query("clip")?,
            [slug] => slug,
            _ => return None,
        };
        return is_id(slug).then(|| video(format!("https://clips.twitch.tv/embed?clip={slug}")));
    }
    match segments.as_slice() {
        ["videos", id] => is_id(id).then(|| video(format!("https://player.twitch.tv/?video={id}"))),
        [_, "clip", slug] => {
            is_id(slug).then(|| video(format!("https://clips.twitch.tv/embed?clip={slug}")))
        }
        [channel] if is_id(channel) => Some(video(format!(
            "https://player.twitch.tv/?channel={channel}"
        ))),
        _ => None,
    }
}

fn facebook(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    if segments.first() == Some(&"plugins") {
        // Already the embed endpoint: kept, so a resolved link resolves to itself.
        let href = parts.query("href")?;
        return Some(match segments.as_slice() {
            ["plugins", "video.php"] => video(parts.as_given()),
            ["plugins", "post.php"] => page(
                format!("https://www.facebook.com/plugins/post.php?href={href}"),
                500.0,
                600.0,
            ),
            _ => return None,
        });
    }
    let original = if parts.host == "fb.watch" {
        let id = segments.first().filter(|id| is_id(id))?;
        format!("https://fb.watch/{id}/")
    } else {
        let is_video = segments.contains(&"videos")
            || segments.contains(&"reel")
            || (segments.first() == Some(&"watch") && parts.query("v").is_some());
        let is_post = segments.contains(&"posts");
        if !is_video && !is_post {
            return None;
        }
        if !segments.iter().all(|s| is_name(s)) {
            return None;
        }
        let mut original = format!("https://www.facebook.com{}", parts.path);
        if let Some(v) = parts.query("v").filter(|v| is_id(v)) {
            original.push_str("?v=");
            original.push_str(v);
        }
        if is_post {
            return Some(page(
                format!(
                    "https://www.facebook.com/plugins/post.php?href={}",
                    encode_component(&original)
                ),
                500.0,
                600.0,
            ));
        }
        original
    };
    Some(video(format!(
        "https://www.facebook.com/plugins/video.php?href={}&show_text=false",
        encode_component(&original)
    )))
}

fn instagram(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let (kind, id) = match segments.as_slice() {
        [kind @ ("p" | "reel" | "tv"), id] | [kind @ ("p" | "reel" | "tv"), id, "embed", ..] => {
            (*kind, *id)
        }
        [_, kind @ ("p" | "reel"), id, ..] => (*kind, *id),
        _ => return None,
    };
    is_id(id).then(|| {
        page(
            format!("https://www.instagram.com/{kind}/{id}/embed"),
            400.0,
            560.0,
        )
    })
}

fn bilibili(parts: Parts<'_>) -> Option<EmbedLink> {
    if parts.host == "player.bilibili.com" {
        let id = parts.query("bvid").filter(|id| is_id(id))?;
        return Some(video(format!(
            "https://player.bilibili.com/player.html?bvid={id}&autoplay=0"
        )));
    }
    let segments = parts.segments();
    let ["video", id, ..] = segments.as_slice() else {
        return None;
    };
    if let Some(aid) = id
        .strip_prefix("av")
        .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))
    {
        return Some(video(format!(
            "https://player.bilibili.com/player.html?aid={aid}&autoplay=0"
        )));
    }
    (id.starts_with("BV") && is_id(id)).then(|| {
        video(format!(
            "https://player.bilibili.com/player.html?bvid={id}&autoplay=0"
        ))
    })
}

fn loom(parts: Parts<'_>) -> Option<EmbedLink> {
    match parts.segments().as_slice() {
        ["share" | "embed", id] if is_id(id) => {
            Some(video(format!("https://www.loom.com/embed/{id}")))
        }
        _ => None,
    }
}

fn ted(parts: Parts<'_>) -> Option<EmbedLink> {
    match parts.segments().as_slice() {
        ["talks", slug] | [_, "talks", slug] if is_id(slug) => {
            Some(video(format!("https://embed.ted.com/talks/{slug}")))
        }
        _ => None,
    }
}

// ----------------------------------------------------------------------- audio

fn spotify(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    // `/intl-fr/track/...` is the same track in another storefront.
    let rest: &[&str] = match segments.as_slice() {
        [first, rest @ ..] if first.starts_with("intl-") => rest,
        all => all,
    };
    let rest = match rest {
        ["embed", rest @ ..] => rest,
        all => all,
    };
    let [kind @ ("track" | "album" | "playlist" | "episode" | "show" | "artist"), id] = rest else {
        return None;
    };
    if !is_id(id) {
        return None;
    }
    let height = if matches!(*kind, "track" | "episode") {
        152.0
    } else {
        352.0
    };
    Some(page(
        format!("https://open.spotify.com/embed/{kind}/{id}"),
        560.0,
        height,
    ))
}

fn soundcloud(parts: Parts<'_>) -> Option<EmbedLink> {
    if parts.host == "w.soundcloud.com" {
        let inner = parts.query("url")?;
        return Some(page(
            format!("https://w.soundcloud.com/player/?url={inner}"),
            560.0,
            166.0,
        ));
    }
    if parts.host != "soundcloud.com" {
        return None;
    }
    let segments = parts.segments();
    let is_set = segments.get(1) == Some(&"sets");
    if segments.len() < 2 || !segments.iter().all(|s| is_name(s)) {
        return None;
    }
    let track = format!("https://soundcloud.com/{}", segments.join("/"));
    Some(page(
        format!(
            "https://w.soundcloud.com/player/?url={}",
            encode_component(&track)
        ),
        560.0,
        if is_set { 450.0 } else { 166.0 },
    ))
}

fn mixcloud(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    if segments.as_slice() == ["widget", "iframe"] {
        let feed = parts.query("feed")?;
        return Some(page(
            format!("https://www.mixcloud.com/widget/iframe/?feed={feed}"),
            560.0,
            120.0,
        ));
    }
    let [user, show] = segments.as_slice() else {
        return None;
    };
    (is_name(user) && is_name(show)).then(|| {
        page(
            format!(
                "https://www.mixcloud.com/widget/iframe/?feed={}",
                encode_component(&format!("/{user}/{show}/"))
            ),
            560.0,
            120.0,
        )
    })
}

fn apple(provider: &str, parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    if segments.len() < 2 || !segments.iter().all(|s| is_name(s)) {
        return None;
    }
    let single = parts.query("i").is_some() || segments.get(1) == Some(&"song");
    let mut url = format!("https://embed.{provider}/{}", segments.join("/"));
    if let Some(i) = parts.query("i").filter(|i| is_id(i)) {
        url.push_str("?i=");
        url.push_str(i);
    }
    Some(page(url, 560.0, if single { 175.0 } else { 450.0 }))
}

// ------------------------------------------------------------------ documents

fn google_drive(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let file_id = match segments.as_slice() {
        ["file", "d", id, ..] => Some(*id),
        ["open"] | ["uc"] => parts.query("id"),
        _ => None,
    }?;
    if !is_id(file_id) {
        return None;
    }
    let mut search = Vec::new();
    if let Some(key) = parts.query("resourcekey").filter(|k| is_id(k)) {
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
    Some(video(format!(
        "https://drive.google.com/file/d/{file_id}/preview{tail}"
    )))
}

fn google_docs(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let base = "https://docs.google.com";
    match segments.as_slice() {
        // Published to the web: its own embeddable address.
        ["document", "d", "e", id, ..] if is_id(id) => Some(page(
            format!("{base}/document/d/e/{id}/pub?embedded=true"),
            560.0,
            720.0,
        )),
        ["presentation", "d", "e", id, ..] if is_id(id) => Some(page(
            format!("{base}/presentation/d/e/{id}/embed"),
            640.0,
            389.0,
        )),
        ["spreadsheets", "d", "e", id, ..] if is_id(id) => Some(page(
            format!("{base}/spreadsheets/d/e/{id}/pubhtml?widget=true&headers=false"),
            640.0,
            480.0,
        )),
        ["forms", "d", "e", id, ..] if is_id(id) => Some(page(
            format!("{base}/forms/d/e/{id}/viewform?embedded=true"),
            DEFAULT_EMBED_WIDTH,
            DEFAULT_EMBED_HEIGHT,
        )),
        // Shared by link: the read-only preview, which is what frames.
        ["document", "d", id, ..] if is_id(id) => Some(page(
            format!("{base}/document/d/{id}/preview"),
            560.0,
            720.0,
        )),
        ["presentation", "d", id, ..] if is_id(id) => Some(page(
            format!("{base}/presentation/d/{id}/embed"),
            640.0,
            389.0,
        )),
        ["spreadsheets", "d", id, ..] if is_id(id) => Some(page(
            format!("{base}/spreadsheets/d/{id}/preview"),
            640.0,
            480.0,
        )),
        _ => None,
    }
}

fn figma(parts: Parts<'_>) -> Option<EmbedLink> {
    let square = |url: String| page(url, 550.0, 550.0);
    if parts.host == "embed.figma.com" || parts.path == "/embed" {
        // Already an embed: resolving it again must not wrap it in another one.
        return parts
            .query("url")
            .map(|_| square(parts.as_given()))
            .or_else(|| (parts.host == "embed.figma.com").then(|| square(parts.as_given())));
    }
    if parts.host != "figma.com" {
        return None;
    }
    // Figma is given the *original* link, encoded, because their embed endpoint takes
    // the file URL as a parameter rather than being a different path.
    let canonical = format!(
        "https://www.figma.com{}{}",
        parts.path,
        if parts.query.is_empty() {
            String::new()
        } else {
            format!("?{}", parts.query)
        }
    );
    Some(square(format!(
        "https://www.figma.com/embed?embed_host=share&url={}",
        encode_component(&canonical)
    )))
}

/// A gist has no address that can be framed: GitHub serves it only as a script that
/// writes the gist into whatever page runs it. So the frame is a small document that
/// runs that script — in a sandbox with no origin, never ours.
fn gist(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let [user, id, ..] = segments.as_slice() else {
        return None;
    };
    let id = id.strip_suffix(".js").unwrap_or(id);
    if !is_id(user) || !is_id(id) {
        return None;
    }
    let url = format!("https://gist.github.com/{user}/{id}");
    let document = format!(
        "<!doctype html><html><head><base target=\"_blank\"><style>\
         *{{margin:0}}html,body{{height:100%}}\
         .gist .gist-file{{height:calc(100vh - 2px);display:grid;grid-template-rows:1fr auto}}\
         .gist .gist-data{{overflow:auto}}\
         </style></head><body><script src=\"{url}.js\"></script></body></html>"
    );
    Some(EmbedLink {
        url,
        intrinsic_width: 550.0,
        intrinsic_height: 720.0,
        kind: EmbedKind::Generic,
        allow_same_origin: false,
        document: Some(document),
    })
}

/// A tweet, framed at the address Twitter's own widget frames it at.
///
/// Excalidraw frames a document that loads `widgets.js`, which then frames this. Going
/// straight to it needs no script running in a document of ours, and works the same.
fn tweet(parts: Parts<'_>) -> Option<EmbedLink> {
    let id = if parts.host == "platform.twitter.com" {
        (parts.path == "/embed/Tweet.html")
            .then(|| parts.query("id"))
            .flatten()?
    } else {
        match parts.segments().as_slice() {
            [_, "status" | "statuses", id, ..] => *id,
            ["i", "web", "status", id, ..] => *id,
            _ => return None,
        }
    };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(page(
        format!("https://platform.twitter.com/embed/Tweet.html?id={id}&dnt=true"),
        480.0,
        560.0,
    ))
}

/// A Reddit post, framed at the address Reddit's own widget frames it at.
fn reddit(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let ["r", sub, "comments", id, rest @ ..] = segments.as_slice() else {
        return None;
    };
    if !is_id(sub) || !is_id(id) {
        return None;
    }
    let slug = rest.first().filter(|s| is_id(s)).copied().unwrap_or("_");
    Some(page(
        format!("https://embed.reddit.com/r/{sub}/comments/{id}/{slug}/?embed=true"),
        560.0,
        480.0,
    ))
}

fn giphy(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let raw = match segments.as_slice() {
        ["gifs" | "clips" | "stickers", slug, ..] => slug,
        ["embed", id] => id,
        ["media", id, ..] | [_, "media", id, ..] => id,
        [file] if parts.host == "i.giphy.com" => file.strip_suffix(".gif").unwrap_or(file),
        _ => return None,
    };
    // A slug is `words-words-ID`: the id is what follows the last hyphen.
    let id = raw.rsplit('-').next()?;
    is_id(id).then(|| page(format!("https://giphy.com/embed/{id}"), 480.0, 360.0))
}

fn val_town(parts: Parts<'_>) -> EmbedLink {
    let segments = parts.segments();
    let url = match segments.as_slice() {
        ["v", name] if is_name(name) => format!("https://val.town/embed/{name}"),
        _ => parts.as_given(),
    };
    page(url, DEFAULT_EMBED_WIDTH, DEFAULT_EMBED_HEIGHT)
}

// ------------------------------------------------------------------------ code

fn codepen(parts: Parts<'_>) -> Option<EmbedLink> {
    let embed = |owner: String, id: &str| {
        page(
            format!("https://codepen.io/{owner}/embed/{id}?default-tab=result"),
            560.0,
            400.0,
        )
    };
    match parts.segments().as_slice() {
        ["team", team, "pen" | "embed" | "full", id, ..] if is_name(team) && is_id(id) => {
            Some(embed(format!("team/{team}"), id))
        }
        [user, "pen" | "embed" | "full", id, ..] if is_name(user) && is_id(id) => {
            Some(embed((*user).to_string(), id))
        }
        _ => None,
    }
}

fn codesandbox(parts: Parts<'_>) -> Option<EmbedLink> {
    let id = match parts.segments().as_slice() {
        ["s" | "embed", id] => *id,
        ["p", "sandbox" | "devbox", id] => *id,
        _ => return None,
    };
    is_id(id).then(|| page(format!("https://codesandbox.io/embed/{id}"), 800.0, 500.0))
}

fn jsfiddle(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let segments: Vec<&str> = segments
        .into_iter()
        .take_while(|s| *s != "embedded" && *s != "show")
        .collect();
    let path = match segments.as_slice() {
        [id] if is_id(id) => (*id).to_string(),
        [user, id] if is_name(user) && is_id(id) => format!("{user}/{id}"),
        [user, id, revision] if is_name(user) && is_id(id) && is_id(revision) => {
            format!("{user}/{id}/{revision}")
        }
        _ => return None,
    };
    Some(page(
        format!("https://jsfiddle.net/{path}/embedded/"),
        560.0,
        400.0,
    ))
}

fn desmos(parts: Parts<'_>) -> Option<EmbedLink> {
    let segments = parts.segments();
    let url = match segments.as_slice() {
        ["calculator", id] if is_id(id) => format!("https://www.desmos.com/calculator/{id}?embed"),
        ["calculator"] | ["geometry"] | ["scientific"] | ["3d"] => parts.as_given(),
        _ => return None,
    };
    Some(page(url, 560.0, 400.0))
}

fn miro(parts: Parts<'_>) -> Option<EmbedLink> {
    match parts.segments().as_slice() {
        ["app", "board" | "live-embed", id, ..] if is_name(&id.replace('=', "")) => Some(page(
            format!("https://miro.com/app/live-embed/{id}/"),
            640.0,
            480.0,
        )),
        _ => None,
    }
}
