//! Resolving a pasted link into something embeddable.
//!
//! This is the one feature where the tests matter more than the code, because every bug
//! in it is either a silently wrong embed or a hole in the page. The allow-list is a
//! security boundary: an iframe pointing at an arbitrary URL is content someone else
//! controls, rendered inside ours, able to cover the whole board.

mod common;
use common::*;
use draw_engine::*;

fn url_of(raw: &str) -> Option<String> {
    embed_link(raw).map(|resolved| resolved.url)
}

// ------------------------------------------------------------------- the boundary

#[test]
fn an_unknown_host_is_not_embedded() {
    for raw in [
        "https://example.com/page",
        "https://evil.test/anything",
        "https://localhost:8080/admin",
        "https://192.168.1.1/",
    ] {
        assert!(embed_link(raw).is_none(), "{raw} was allowed");
    }
}

#[test]
fn a_host_that_merely_ends_with_an_allowed_one_is_refused() {
    // The trap a naive `endsWith` falls into, and the reason matching is on a dot
    // boundary: `notyoutube.com` and `youtube.com.evil.test` are not YouTube.
    for raw in [
        "https://notyoutube.com/watch?v=abc",
        "https://myyoutube.com/watch?v=abc",
        "https://youtube.com.evil.test/watch?v=abc",
        "https://fakefigma.com/file/x",
    ] {
        assert!(embed_link(raw).is_none(), "{raw} was allowed");
    }
}

#[test]
fn a_subdomain_of_an_allowed_host_is_allowed() {
    // Needed for `player.vimeo.com`, and for the wildcard entry — and it is the reason
    // the check cannot simply be equality.
    assert!(is_allowed_embed_host("player.vimeo.com"));
    assert!(is_allowed_embed_host("docs.simplepdf.eu"));
    assert!(is_allowed_embed_host("www.youtube.com"));
    assert!(!is_allowed_embed_host("vimeo.com.evil.test"));
}

#[test]
fn a_scheme_that_is_not_the_web_is_refused() {
    // The hole the allow-list exists to close. `javascript:` in a frame executes, and a
    // `data:` document inherits nothing useful but can still cover the board.
    for raw in [
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "file:///etc/passwd",
        "ftp://youtube.com/watch?v=abc",
    ] {
        assert!(embed_link(raw).is_none(), "{raw} was allowed");
    }
}

#[test]
fn credentials_in_a_url_do_not_smuggle_a_host_past_the_check() {
    // `https://youtube.com@evil.test/` is a request to **evil.test**. Reading the host
    // as everything before the first `/` would see youtube.com and wave it through.
    assert!(embed_link("https://youtube.com@evil.test/x").is_none());
    assert!(embed_link("https://youtube.com:pass@evil.test/x").is_none());
}

#[test]
fn nothing_is_embedded_from_nothing() {
    for raw in ["", "   ", "not a url", "/relative/path"] {
        assert!(embed_link(raw).is_none(), "{raw:?} was allowed");
    }
}

#[test]
fn only_the_providers_that_need_it_keep_their_origin() {
    // A same-origin frame can reach the embedding page's storage. Off is the default,
    // and on is a decision made per provider.
    let youtube = embed_link("https://youtube.com/watch?v=abc").expect("youtube");
    assert!(youtube.allow_same_origin);

    let gist = embed_link("https://gist.github.com/someone/1234").expect("gist");
    assert!(
        !gist.allow_same_origin,
        "a gist does not need our origin and must not have it"
    );
}

// ---------------------------------------------------------------------- youtube

#[test]
fn a_youtube_watch_page_becomes_a_player() {
    // The whole reason links are rewritten: a watch page in an iframe shows a refusal,
    // not a video, and that looks like our bug.
    assert_eq!(
        url_of("https://www.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(),
        Some("https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1")
    );
}

#[test]
fn every_way_of_writing_a_youtube_link_lands_on_the_same_player() {
    for raw in [
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "http://youtube.com/watch?v=dQw4w9WgXcQ",
        "youtube.com/watch?v=dQw4w9WgXcQ",
        "https://youtu.be/dQw4w9WgXcQ",
        "youtu.be/dQw4w9WgXcQ",
        "https://www.youtube.com/embed/dQw4w9WgXcQ",
    ] {
        assert_eq!(
            url_of(raw).as_deref(),
            Some("https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1"),
            "{raw}"
        );
    }
}

#[test]
fn a_youtube_short_is_portrait() {
    // A short in a 16:9 box is two black bars and a stripe of video. The shape is the
    // point of carrying an intrinsic size at all.
    let short = embed_link("https://youtube.com/shorts/abc123").expect("short");
    assert!(
        short.intrinsic_height > short.intrinsic_width,
        "{}x{}",
        short.intrinsic_width,
        short.intrinsic_height
    );

    let landscape = embed_link("https://youtube.com/watch?v=abc123").expect("video");
    assert!(landscape.intrinsic_width > landscape.intrinsic_height);
}

#[test]
fn a_playlist_becomes_a_playlist_player() {
    assert_eq!(
        url_of("https://www.youtube.com/playlist?list=PL1234").as_deref(),
        Some("https://www.youtube.com/embed/videoseries?list=PL1234&enablejsapi=1")
    );
}

#[test]
fn a_video_shared_from_a_playlist_keeps_the_playlist() {
    // Otherwise the player shows the one video, with nothing to step through after it.
    for link in [
        "https://www.youtube.com/watch?v=abc&list=PL1234",
        "https://youtu.be/abc?list=PL1234",
    ] {
        assert_eq!(
            url_of(link).as_deref(),
            Some("https://www.youtube.com/embed/abc?list=PL1234&enablejsapi=1"),
            "{link}"
        );
    }
    // Stored resolved and resolved again on every frame: that has to be a fixed point.
    let stored = "https://www.youtube.com/embed/abc?list=PL1234&enablejsapi=1";
    assert_eq!(url_of(stored).as_deref(), Some(stored));
}

#[test]
fn a_timestamp_survives_the_rewrite() {
    // Someone who shared a link at 2:03 meant 2:03.
    assert_eq!(
        url_of("https://youtu.be/abc?t=123").as_deref(),
        Some("https://www.youtube.com/embed/abc?enablejsapi=1&start=123")
    );
    assert_eq!(
        url_of("https://youtu.be/abc?t=1h2m3s").as_deref(),
        Some("https://www.youtube.com/embed/abc?enablejsapi=1&start=3723")
    );
}

#[test]
fn the_timestamp_parser_refuses_what_it_does_not_understand() {
    // Guessing would start the video somewhere arbitrary, which is worse than starting
    // it at the beginning.
    assert_eq!(parse_timestamp("t=90"), 90);
    assert_eq!(parse_timestamp("start=45"), 45);
    assert_eq!(parse_timestamp("t=2m"), 120);
    assert_eq!(parse_timestamp("t=1h"), 3600);
    assert_eq!(parse_timestamp(""), 0);
    assert_eq!(parse_timestamp("t=later"), 0);
    assert_eq!(parse_timestamp("t=-5"), 0);
    assert_eq!(parse_timestamp("v=abc"), 0);
}

#[test]
fn a_youtube_link_with_no_video_is_not_an_embed() {
    // The channel page, the home page: allowed hosts, but nothing to play. Framing them
    // produces an empty box rather than a video.
    assert!(embed_link("https://youtube.com/watch").is_none());
    assert!(embed_link("https://youtube.com/playlist").is_none());
}

// ------------------------------------------------------------------------ vimeo

#[test]
fn a_vimeo_link_becomes_its_player() {
    assert_eq!(
        url_of("https://vimeo.com/123456789").as_deref(),
        Some("https://player.vimeo.com/video/123456789?api=1")
    );
    assert_eq!(
        url_of("https://player.vimeo.com/video/123456789").as_deref(),
        Some("https://player.vimeo.com/video/123456789?api=1")
    );
}

#[test]
fn a_vimeo_link_that_is_not_a_video_id_is_refused() {
    // Their player takes a number. A slug would load a page that refuses to frame, which
    // again looks like our bug rather than a bad link.
    assert!(embed_link("https://vimeo.com/channels/staffpicks").is_none());
    assert!(embed_link("https://vimeo.com/").is_none());
}

// ----------------------------------------------------------------------- figma

#[test]
fn a_figma_file_is_passed_to_their_embed_endpoint() {
    let url = url_of("https://www.figma.com/file/abc/Design").expect("figma");
    assert!(url.starts_with("https://www.figma.com/embed?embed_host=share&url="));
    // Encoded, so the inner URL's own separators cannot end the parameter early.
    assert!(url.contains("https%3A%2F%2Fwww.figma.com%2Ffile%2Fabc%2FDesign"));
}

#[test]
fn a_figma_url_with_its_own_query_is_encoded_whole() {
    // The case that a looser encoding gets wrong: an unencoded `&` ends the `url`
    // parameter, and Figma is handed a truncated link that loads the wrong file.
    let url = url_of("https://www.figma.com/file/abc?node-id=1%3A2&mode=design").expect("figma");
    assert!(
        !url.trim_start_matches("https://www.figma.com/embed?embed_host=share&url=")
            .contains('&'),
        "the inner URL leaked a separator: {url}"
    );
}

#[test]
fn a_figma_embed_is_square_rather_than_a_video_shape() {
    let figma = embed_link("https://figma.com/file/abc").expect("figma");
    assert_eq!(figma.kind, EmbedKind::Generic);
    assert_close(figma.intrinsic_width, figma.intrinsic_height);
}

// ---------------------------------------------------------------- google drive

#[test]
fn a_google_drive_file_becomes_a_preview() {
    assert_eq!(
        url_of("https://drive.google.com/file/d/FILE123/view").as_deref(),
        Some("https://drive.google.com/file/d/FILE123/preview")
    );
    assert_eq!(
        url_of("https://drive.google.com/open?id=FILE123").as_deref(),
        Some("https://drive.google.com/file/d/FILE123/preview")
    );
}

#[test]
fn a_drive_resource_key_is_kept() {
    // Link-shared files need it; dropping it turns a working link into a permission
    // error that looks like the file was deleted.
    assert_eq!(
        url_of("https://drive.google.com/file/d/FILE123/view?resourcekey=0-abc").as_deref(),
        Some("https://drive.google.com/file/d/FILE123/preview?resourcekey=0-abc")
    );
}

#[test]
fn a_drive_id_that_is_not_an_id_is_refused() {
    // The id goes straight into a URL we construct, so anything that is not an id is a
    // chance to build a URL pointing somewhere else.
    assert!(embed_link("https://drive.google.com/file/d/..%2F..%2Fadmin/view").is_none());
    assert!(embed_link("https://drive.google.com/open?id=a/b").is_none());
}

// ---------------------------------------------------------------- through the engine

#[test]
fn inserting_an_embed_stores_the_resolved_url_rather_than_the_pasted_one() {
    // Resolved once, at insert. Storing the pasted link would make the board depend on
    // the rules still agreeing when it is next opened.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);

    let id = engine
        .insert_embed("https://youtu.be/dQw4w9WgXcQ", 600.0, 450.0)
        .expect("embed was not inserted");
    let element = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("embed vanished");

    assert_eq!(element.kind, DrawElementType::Embed);
    assert_eq!(
        element.embed_url.as_deref(),
        Some("https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1")
    );
}

#[test]
fn an_embed_arrives_in_the_providers_own_shape() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let id = engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");
    let element = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("embed vanished");

    assert_near(
        element.width / element.height,
        VIDEO_EMBED_WIDTH / VIDEO_EMBED_HEIGHT,
        1e-6,
    );
}

#[test]
fn a_link_that_cannot_be_embedded_puts_nothing_on_the_board() {
    // An empty box where a video was expected is worse than being told the link will not
    // work, and it is one more thing to delete.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);

    assert!(engine
        .insert_embed("https://example.com/page", 600.0, 450.0)
        .is_none());
    assert!(engine.get_scene().iter().all(|el| el.is_deleted));
}

#[test]
fn the_embed_is_reachable_by_its_own_key() {
    assert_eq!(tool_for_key("w"), Some(DrawTool::Embed));
    assert_eq!(tool_for_key("W"), Some(DrawTool::Embed));
}

fn assert_near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "expected {actual} within {tolerance} of {expected}"
    );
}

// ------------------------------------------------------------------ live frames

#[test]
fn an_embed_reports_where_its_frame_goes_in_screen_pixels() {
    // The host positions a real `<iframe>` over the canvas. Doing the camera conversion
    // itself is how the frame ends up drifting away from the rectangle drawn under it.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");

    let frames = engine.embed_frames();
    assert_eq!(frames.len(), 1);
    let frame = &frames[0];
    assert_close(frame.x + frame.width / 2.0, 600.0);
    assert_close(frame.y + frame.height / 2.0, 450.0);
    assert_eq!(frame.url, "https://www.youtube.com/embed/abc?enablejsapi=1");
    assert!(frame.allow_same_origin, "youtube's player needs its origin");
}

#[test]
fn a_frames_box_follows_the_camera() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");

    let before = engine.embed_frames()[0].clone();
    engine.pan_by(100.0, 50.0);
    let after = engine.embed_frames()[0].clone();

    assert_close(after.x - before.x, 100.0);
    assert_close(after.y - before.y, 50.0);
    assert_close(after.width, before.width);

    engine.zoom_at(600.0, 450.0, 2.0);
    let zoomed = engine.embed_frames()[0].clone();
    assert_near(zoomed.width / before.width, 2.0, 1e-6);
}

#[test]
fn an_embed_scrolled_out_of_view_keeps_its_frame_but_says_so() {
    // Still reported: dropping the frame made the host unmount a playing video the
    // moment it was panned away. `visible` is what lets the host not load a player
    // nobody has seen yet — thirty videos on a board, one of them on screen.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");
    assert!(engine.embed_frames()[0].visible);

    engine.pan_by(-5000.0, 0.0);
    let frames = engine.embed_frames();
    assert_eq!(frames.len(), 1, "an embed off screen lost its frame");
    assert!(
        !frames[0].visible,
        "an embed far off screen reads as visible"
    );
}

#[test]
fn an_embed_can_be_pointed_at_another_link() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let id = engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");
    let before = engine.embed_frames()[0].clone();

    assert!(engine.set_embed_url(&id, "https://vimeo.com/123456"));
    let after = engine.embed_frames()[0].clone();
    assert_eq!(after.url, "https://player.vimeo.com/video/123456?api=1");
    // The box is where the person put it and the size they made it.
    assert_close(after.x, before.x);
    assert_close(after.width, before.width);

    // Undo is how a wrong link is taken back.
    engine.undo();
    assert_eq!(engine.embed_frames()[0].url, before.url);
}

#[test]
fn a_link_the_rules_refuse_leaves_the_embed_alone() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let id = engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");
    assert!(!engine.set_embed_url(&id, "https://evil.example/"));
    assert!(!engine.set_embed_url(&id, "javascript:alert(1)"));
    assert_eq!(
        engine.embed_frames()[0].url,
        "https://www.youtube.com/embed/abc?enablejsapi=1"
    );
}

#[test]
fn a_deleted_embed_reports_no_frame() {
    // Otherwise the page keeps playing behind a board that no longer shows it.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let id = engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");

    engine.select(vec![id]);
    engine.delete_selection();
    assert!(engine.embed_frames().is_empty());
}

// ------------------------------------------------------------- every provider

/// Pasted link → the address that is framed. One row per way of writing a link that a
/// share button or an address bar actually produces.
const REWRITES: &[(&str, &str)] = &[
    // YouTube, beyond the watch page.
    ("https://m.youtube.com/watch?v=dQw4w9WgXcQ", "https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1"),
    ("https://music.youtube.com/watch?v=dQw4w9WgXcQ&feature=share", "https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1"),
    ("https://www.youtube.com/watch?feature=shared&v=dQw4w9WgXcQ", "https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1"),
    ("https://youtu.be/dQw4w9WgXcQ?si=abcDEF123", "https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1"),
    ("https://www.youtube.com/live/jfKfPfyJRdk?si=x", "https://www.youtube.com/embed/jfKfPfyJRdk?enablejsapi=1"),
    ("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ", "https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1"),
    ("https://www.youtube.com/v/dQw4w9WgXcQ", "https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1"),
    // Vimeo: channels, the player, and an unlisted video's hash.
    ("https://vimeo.com/channels/staffpicks/76979871", "https://player.vimeo.com/video/76979871?api=1"),
    ("https://vimeo.com/76979871/abc123def0", "https://player.vimeo.com/video/76979871?api=1&h=abc123def0"),
    ("https://player.vimeo.com/video/76979871?h=abc123def0", "https://player.vimeo.com/video/76979871?api=1&h=abc123def0"),
    // Google.
    ("https://docs.google.com/document/d/1AbC_d-EF/edit?usp=sharing", "https://docs.google.com/document/d/1AbC_d-EF/preview"),
    ("https://docs.google.com/presentation/d/1AbC/edit#slide=id.p", "https://docs.google.com/presentation/d/1AbC/embed"),
    ("https://docs.google.com/spreadsheets/d/1AbC/edit#gid=0", "https://docs.google.com/spreadsheets/d/1AbC/preview"),
    ("https://docs.google.com/forms/d/e/1FAIpQL/viewform?usp=sf_link", "https://docs.google.com/forms/d/e/1FAIpQL/viewform?embedded=true"),
    ("https://docs.google.com/document/d/e/2PACX-1v/pub", "https://docs.google.com/document/d/e/2PACX-1v/pub?embedded=true"),
    ("https://www.google.com/maps/embed?pb=!1m18!1m12", "https://www.google.com/maps/embed?pb=!1m18!1m12"),
    ("https://calendar.google.com/calendar/embed?src=abc%40group.calendar.google.com", "https://calendar.google.com/calendar/embed?src=abc%40group.calendar.google.com"),
    // Social.
    ("https://twitter.com/jack/status/20", "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true"),
    ("https://x.com/jack/status/20?s=46&t=abc", "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true"),
    ("https://mobile.twitter.com/jack/status/20", "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true"),
    ("https://www.reddit.com/r/rust/comments/abc123/some_title/", "https://embed.reddit.com/r/rust/comments/abc123/some_title/?embed=true"),
    ("https://old.reddit.com/r/rust/comments/abc123/", "https://embed.reddit.com/r/rust/comments/abc123/_/?embed=true"),
    ("https://www.instagram.com/p/CvRHtJvLTdy/", "https://www.instagram.com/p/CvRHtJvLTdy/embed"),
    ("https://www.instagram.com/reel/CvRHtJvLTdy/?igsh=x", "https://www.instagram.com/reel/CvRHtJvLTdy/embed"),
    ("https://www.facebook.com/facebook/videos/10153231379946729/", "https://www.facebook.com/plugins/video.php?href=https%3A%2F%2Fwww.facebook.com%2Ffacebook%2Fvideos%2F10153231379946729%2F&show_text=false"),
    ("https://www.tiktok.com/@scout2015/video/6718335390845095173", "https://www.tiktok.com/embed/v2/6718335390845095173"),
    ("https://giphy.com/gifs/cat-funny-JIX9t2j0ZTN9S", "https://giphy.com/embed/JIX9t2j0ZTN9S"),
    ("https://media.giphy.com/media/JIX9t2j0ZTN9S/giphy.gif", "https://giphy.com/embed/JIX9t2j0ZTN9S"),
    // Video and audio.
    ("https://www.loom.com/share/e5b8c04bca094dd8a5507925ab887002?sid=1", "https://www.loom.com/embed/e5b8c04bca094dd8a5507925ab887002"),
    ("https://www.dailymotion.com/video/x8abcd_some-title", "https://www.dailymotion.com/embed/video/x8abcd"),
    ("https://dai.ly/x8abcd", "https://www.dailymotion.com/embed/video/x8abcd"),
    ("https://streamable.com/moo", "https://streamable.com/e/moo"),
    ("https://www.twitch.tv/monstercat", "https://player.twitch.tv/?channel=monstercat"),
    ("https://www.twitch.tv/videos/123456", "https://player.twitch.tv/?video=123456"),
    ("https://clips.twitch.tv/AwkwardClip-abc", "https://clips.twitch.tv/embed?clip=AwkwardClip-abc"),
    ("https://www.bilibili.com/video/BV1GJ411x7h7/?spm_id_from=x", "https://player.bilibili.com/player.html?bvid=BV1GJ411x7h7&autoplay=0"),
    ("https://www.ted.com/talks/ken_robinson_do_schools_kill_creativity", "https://embed.ted.com/talks/ken_robinson_do_schools_kill_creativity"),
    ("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT?si=x", "https://open.spotify.com/embed/track/4cOdK2wGLETKBW3PvgPWqT"),
    ("https://open.spotify.com/intl-fr/album/6XhjNHCyCDyyGJRM5mg40G", "https://open.spotify.com/embed/album/6XhjNHCyCDyyGJRM5mg40G"),
    ("https://soundcloud.com/forss/flickermood", "https://w.soundcloud.com/player/?url=https%3A%2F%2Fsoundcloud.com%2Fforss%2Fflickermood"),
    ("https://www.mixcloud.com/spartacus/party-time/", "https://www.mixcloud.com/widget/iframe/?feed=%2Fspartacus%2Fparty-time%2F"),
    ("https://podcasts.apple.com/us/podcast/the-daily/id1200361736", "https://embed.podcasts.apple.com/us/podcast/the-daily/id1200361736"),
    ("https://music.apple.com/us/album/whenever/1440818839?i=1440819032", "https://embed.music.apple.com/us/album/whenever/1440818839?i=1440819032"),
    // Code and tools.
    ("https://gist.github.com/octocat/6cad326836d38bd3a7ae", "https://gist.github.com/octocat/6cad326836d38bd3a7ae"),
    ("https://codepen.io/team/codepen/pen/PNaGbb", "https://codepen.io/team/codepen/embed/PNaGbb?default-tab=result"),
    ("https://codesandbox.io/s/new", "https://codesandbox.io/embed/new"),
    ("https://codesandbox.io/p/sandbox/react-abc123", "https://codesandbox.io/embed/react-abc123"),
    ("https://jsfiddle.net/zalun/NmudS/", "https://jsfiddle.net/zalun/NmudS/embedded/"),
    ("https://stackblitz.com/edit/vitejs-vite", "https://stackblitz.com/edit/vitejs-vite?embed=1"),
    ("https://www.val.town/v/stevekrouse.whatIsValTown", "https://val.town/embed/stevekrouse.whatIsValTown"),
    ("https://www.desmos.com/calculator/zukjgk9iry", "https://www.desmos.com/calculator/zukjgk9iry?embed"),
    ("https://miro.com/app/board/uXjVOfjmJQo=/", "https://miro.com/app/live-embed/uXjVOfjmJQo=/"),
    ("https://forms.office.com/r/abc123", "https://forms.office.com/r/abc123?embed=true"),
    ("https://www.openstreetmap.org/export/embed.html?bbox=1,2,3,4&layer=mapnik", "https://www.openstreetmap.org/export/embed.html?bbox=1,2,3,4&layer=mapnik"),
];

#[test]
fn every_provider_rewrites_its_share_link_to_its_player() {
    for (raw, expected) in REWRITES {
        assert_eq!(url_of(raw).as_deref(), Some(*expected), "{raw}");
    }
}

#[test]
fn a_resolved_link_resolves_to_itself() {
    // The board stores what this returns and resolves it again every time the frame is
    // shown. A rewrite that did not survive a second pass would wrap an embed in another
    // embed each time the board was opened.
    let pasted = REWRITES.iter().map(|(raw, _)| *raw).chain([
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=90",
        "https://www.youtube.com/playlist?list=PL1234",
        "https://www.figma.com/file/abc/Design?node-id=1%3A2",
        "https://drive.google.com/file/d/FILE123/view?resourcekey=0-abc",
    ]);
    for raw in pasted {
        let once = embed_link(raw).unwrap_or_else(|| panic!("{raw} was refused"));
        let twice = embed_link(&once.url).unwrap_or_else(|| panic!("{} was refused", once.url));
        assert_eq!(twice.url, once.url, "{raw}");
        assert_eq!(twice.document, once.document, "{raw}");
        assert_eq!(twice.kind, once.kind, "{raw}");
    }
}

#[test]
fn a_page_of_a_provider_that_is_not_a_post_is_refused() {
    // These hosts are allowed, but their own pages refuse to be framed: a profile or a
    // front page would arrive as an empty box that looks like our bug.
    for raw in [
        "https://twitter.com/jack",
        "https://x.com/home",
        "https://www.reddit.com/r/rust/",
        "https://www.instagram.com/instagram/",
        "https://www.facebook.com/facebook",
        "https://www.tiktok.com/@scout2015",
        "https://open.spotify.com/user/spotify",
        "https://docs.google.com/",
        "https://www.google.com/search?q=x",
        "https://mail.google.com/mail/u/0/",
        "https://www.loom.com/looms/videos",
        "https://codepen.io/trending",
    ] {
        assert!(embed_link(raw).is_none(), "{raw} was allowed");
    }
}

#[test]
fn an_id_that_is_not_an_id_is_refused_everywhere() {
    // Every id goes straight into a URL on someone else's domain. A path or a query
    // smuggled in as an id would build a link the rules never looked at.
    for raw in [
        "https://youtu.be/..%2F..%2Fx",
        "https://www.youtube.com/watch?v=abc%26list%3Dx",
        "https://twitter.com/jack/status/20abc",
        "https://www.loom.com/share/abc%2F..",
        "https://open.spotify.com/track/a.b",
        "https://streamable.com/e/a%2Fb",
        "https://www.tiktok.com/@a/video/12x",
        "https://codesandbox.io/s/a%3Fb",
    ] {
        assert!(embed_link(raw).is_none(), "{raw} was allowed");
    }
}

#[test]
fn a_gist_is_a_document_that_runs_its_script_without_our_origin() {
    // GitHub serves a gist only as a script that writes it into the page running it, so
    // there is no address to frame — and a document we frame would have *our* origin.
    // It gets none: the script runs in a sandbox that cannot reach the board.
    let gist = embed_link("https://gist.github.com/octocat/6cad326836d38bd3a7ae").expect("gist");
    let document = gist.document.expect("a gist is a document");
    assert!(document.contains(
        "<script src=\"https://gist.github.com/octocat/6cad326836d38bd3a7ae.js\"></script>"
    ));
    assert!(!gist.allow_same_origin);
}

#[test]
fn only_a_document_is_denied_its_origin() {
    // A frame loaded from the provider's address has the provider's origin, not ours, so
    // letting it keep that origin reaches nothing of ours — and players need it.
    for (raw, _) in REWRITES {
        let link = embed_link(raw).expect("resolved");
        assert_eq!(link.allow_same_origin, link.document.is_none(), "{raw}");
    }
}

#[test]
fn a_pasted_snippet_is_resolved_by_the_link_inside_it() {
    // What a provider's "embed" button copies. Only the link is taken out of it, and the
    // link goes through the same rules as one pasted on its own.
    let cases = [
        (
            r#"<iframe width="560" height="315" src="https://www.youtube.com/embed/dQw4w9WgXcQ?si=x" title="YouTube video player" frameborder="0" allowfullscreen></iframe>"#,
            "https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1",
        ),
        (
            r#"<blockquote class="twitter-tweet"><p lang="en">just setting up my twttr</p>&mdash; jack (@jack) <a href="https://twitter.com/jack/status/20?ref_src=twsrc">March 21, 2006</a></blockquote>"#,
            "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true",
        ),
        (
            r#"<script src="https://gist.github.com/octocat/6cad326836d38bd3a7ae.js"></script>"#,
            "https://gist.github.com/octocat/6cad326836d38bd3a7ae",
        ),
        (
            r#"<iframe src="https://player.vimeo.com/video/76979871?h=8272103f6e" width="640" height="360"></iframe>"#,
            "https://player.vimeo.com/video/76979871?api=1&h=8272103f6e",
        ),
    ];
    for (snippet, expected) in cases {
        assert_eq!(url_of(snippet).as_deref(), Some(expected), "{snippet}");
    }
    // A snippet framing somewhere not allowed is refused like the bare link would be.
    assert!(embed_link(r#"<iframe src="https://evil.test/x"></iframe>"#).is_none());
}

#[test]
fn a_frame_is_resolved_again_so_an_old_board_gets_the_current_rules() {
    // A board saved when tweets were framed at their own page — which Twitter refuses —
    // shows the tweet once it is opened under the current rules.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let id = engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");
    let mut scene = engine.get_scene();
    for element in &mut scene {
        if element.id == id {
            element.embed_url = Some("https://twitter.com/jack/status/20".into());
        }
    }
    engine.set_scene(Scene::new(scene));

    let frames = engine.embed_frames();
    assert_eq!(
        frames[0].url,
        "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true"
    );
}

#[test]
fn a_frame_says_what_it_is_and_how_far_the_board_is_zoomed() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("video");
    engine
        .insert_embed(
            "https://gist.github.com/octocat/6cad326836d38bd3a7ae",
            300.0,
            300.0,
        )
        .expect("gist");
    engine.zoom_at(600.0, 450.0, 0.5);

    let frames = engine.embed_frames();
    let video = frames
        .iter()
        .find(|f| f.url.contains("youtube"))
        .expect("video frame");
    assert_eq!(video.kind, "video");
    assert!(video.srcdoc.is_none());
    assert_near(video.scale, engine.camera.scale, 1e-12);
    let gist = frames
        .iter()
        .find(|f| f.url.contains("gist"))
        .expect("gist frame");
    assert_eq!(gist.kind, "generic");
    assert!(gist.srcdoc.is_some() && !gist.allow_same_origin);
}

// ------------------------------------------------------------------------ locking

/// An embed locks as any element does: `toggle_lock_selection` has no kind filter. Locked,
/// a press on it picks nothing up; a second toggle, from a right-click, gives it back.
#[test]
fn a_locked_embed_stays_put_until_it_is_unlocked() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let id = engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");
    let x_of = |engine: &DrawEngine| {
        engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == id)
            .expect("embed vanished")
            .x
    };
    let drag = |engine: &mut DrawEngine| {
        let embed = engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == id)
            .unwrap();
        // On its top edge, which any shape is hit on.
        let (x, y) = (embed.x + embed.width / 2.0, embed.y);
        engine.begin_pointer(x, y, false, false);
        engine.move_pointer(x + 100.0, y, false, false);
        engine.end_pointer();
    };
    engine.select(vec![id.clone()]);
    engine.toggle_lock_selection();
    engine.clear_selection();
    let before = x_of(&engine);

    drag(&mut engine);
    assert_close(x_of(&engine), before);

    engine.select_element(&id);
    engine.toggle_lock_selection();
    engine.clear_selection();
    drag(&mut engine);
    assert_close(x_of(&engine), before + 100.0);
}
