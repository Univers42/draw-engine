//! Runtime-agnostic drawing engine core. WASM bindings live in `wasm`.

pub mod camera;
pub mod edit;
pub mod engine;
pub mod export;
pub mod freehand;
pub mod history;
pub mod interaction;
pub mod math;
pub mod render;
pub mod scene;
pub mod selection;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use camera::{
    fit_bounds, normalize_zoom, pan_by, screen_to_world, visible_world_rect, wheel_zoom_scale,
    world_to_screen, zoom_at, zoom_to, Camera, Point, WorldBounds, IDENTITY, MAX_ZOOM, MIN_ZOOM,
    ZOOM_STEP,
};
pub use edit::{
    align_elements, distribute_elements, expand_for_copy, expand_for_copy_among, expand_to_groups,
    expand_to_groups_among, flip_elements, group_patches, is_single_group, materialize,
    materialize_elements, reorder_elements, serialize_selection, ungroup_patches, AlignMode,
    FlipAxis, ZOrderMode,
};
pub use engine::{
    DrawEngine, EmbedFrame, EngineEvents, HoverCursor, NoopPainter, Notice, PaintView, Painter,
    TextEditRequest,
};
pub use export::{elements_from_json, scene_to_json, scene_to_svg, OsidrawFile};
pub use freehand::points_bounds;
pub use history::SnapshotHistory;
pub use interaction::{
    constrain_to_angle, ease_out, is_degenerate_linear, is_degenerate_rect, is_linear_tool,
    is_shape_tool, is_toggle_tool, linear_from_drag, rect_from_drag, size_mapping, snap_move,
    tool_for_chord, tool_for_key, Axis, DrawTool, LaserOptions, LaserPoint, LaserStroke,
    LaserTrails, LinearDrag, SnapGuide, SnapResult, ALL_TOOLS, CORNER_DETECTION_MAX_ANGLE_DEG,
    DEFAULT_LASER_COLOR, LASER_DECAY_LENGTH, LASER_DECAY_TIME_MS, LASER_MAX_TAIL_LENGTH,
    LASER_SIZE, LASER_STREAMLINE,
};
pub use math::{
    bezier_point, bezier_point_at_fraction, catmull_rom_cubics, clamp, hash_string, lerp, round_px,
    smoothstep, Cubic, CURVE_TIGHTNESS,
};
pub use render::{
    canvas_text_align, dark_theme, default_arrowhead, font_string, is_roughable, label_offset_y,
    light_theme, text_anchor_x, DrawTheme, GridSettings, DEFAULT_GRID_SIZE, DEFAULT_GRID_STEP,
    FONT_FAMILY, TEXT_LINE_HEIGHT,
};
pub use scene::{
    apply_style_patch, attach_point, bindable_at, bump_version, create_element,
    create_element_default, default_element_style, element_bounds, element_center,
    element_rotated_bounds, hit_test, hit_test_element, is_auto_resize, is_bindable_element,
    is_binding_element, is_linear_element, is_path_a_loop_within, is_transparent, layout_label,
    linear_endpoints, linear_from_endpoints, linear_retarget, local_box, local_center, merge_style,
    new_element_id, normalize_rect, refresh_bindings, resolved_text_align, resolved_vertical_align,
    rotation_center, scene_bounds, segment_hits_element, Arrowhead, DrawElement, DrawElementStyle,
    DrawElementStylePatch, DrawElementType, FillStyle, Geometry, Rect, Scene, StrokeStyle,
    TextAlign, VerticalAlign, ARROWHEADS, BINDING_GAP, LABEL_PADDING, TEXT_ALIGNS, VERTICAL_ALIGNS,
};
pub use scene::{
    arrow_endpoints, classify, convex_hull, elongation, extract_features, kurtosis, polygon_area,
    principal_axes, principal_coords, recognize_shape, resample, skewness, standardized_moment,
    PrincipalAxes, RecognizedShape, StrokeFeatures, ARROW_MIN_SKEW, CLOSED_GAP_MAX_RATIO,
    CLOSED_SHAPE_MAX_DISTANCE, LINEAR_MAX_ELONGATION, LINEAR_MAX_SHAFT_DEVIATION,
    RECOGNITION_MIN_SCREEN_SIZE, RESAMPLE_N,
};
pub use scene::{
    compute_bucket_fill, is_bucket_fill_compatible, is_restylable_fill, polygon_includes_point,
    polygon_includes_point_non_zero, polygon_signed_area, renders_opaque_fill,
    segment_intersection_point, BucketFill, BucketFillFailure, BucketFillInsertion,
    BucketFillOptions, Placement, BUCKET_FILL_COVER_MARGIN, BUCKET_FILL_GAP_TOLERANCE,
    BUCKET_FILL_REGION_MATCH_TOLERANCE, LINE_CONFIRM_THRESHOLD,
};
pub use scene::{
    default_frame_name, element_contains_frame, element_in_frame_bounds, element_intersects_frame,
    element_outline, element_overlaps_frame, elements_captured_by, fit_image, frame_children,
    frame_clip_bounds, frame_for_element, frame_name_anchor, frame_style, is_frame, is_image,
    locks_aspect_ratio, needs_frame_clip, outline_edges, outline_is_closed, segments_intersect,
    ImageFit, FRAME_MIN_SIZE, FRAME_NAME_COLOR_DARK, FRAME_NAME_COLOR_LIGHT, FRAME_NAME_FONT_SIZE,
    FRAME_NAME_LINE_HEIGHT, FRAME_NAME_OFFSET_Y, FRAME_RADIUS, FRAME_STROKE, FRAME_STROKE_WIDTH,
    IMAGE_MIN_VIEWPORT_HEIGHT, IMAGE_VIEWPORT_FRACTION, IMAGE_VIEWPORT_MARGIN,
};
pub use scene::{
    embed_link, embed_src_of, is_allowed_embed_host, parse_timestamp, EmbedKind, EmbedLink,
    ALLOWED_EMBED_HOSTS, DEFAULT_EMBED_HEIGHT, DEFAULT_EMBED_WIDTH, VIDEO_EMBED_HEIGHT,
    VIDEO_EMBED_WIDTH,
};
pub use selection::{
    elements_in_lasso, elements_in_marquee, elements_in_marquee_among, handle_local_point,
    hit_handle, marquee_rect, polygon_contains_point, resize_element, rotate_element, rotate_point,
    selection_corners, selection_corners_padded, selection_handle_points, selection_handles,
    simplify_path, HandleKind, HandleLayout, HandlePoint, LassoMode, RESIZE_HANDLES,
};
