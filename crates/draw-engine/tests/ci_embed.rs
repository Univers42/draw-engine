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
fn an_embed_scrolled_out_of_view_reports_no_frame() {
    // An off-screen `<iframe>` is a page still running. A board with thirty videos
    // should not have thirty players loaded because one of them is visible.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    engine
        .insert_embed("https://youtu.be/abc", 600.0, 450.0)
        .expect("embed was not inserted");
    assert_eq!(engine.embed_frames().len(), 1);

    engine.pan_by(-5000.0, 0.0);
    assert!(
        engine.embed_frames().is_empty(),
        "an embed far off screen still asked for a frame"
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
