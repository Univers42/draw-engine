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
    fit_bounds, pan_by, screen_to_world, visible_world_rect, world_to_screen, zoom_at, zoom_to,
    Camera, Point, WorldBounds, IDENTITY, MAX_ZOOM, MIN_ZOOM,
};
pub use edit::{
    align_elements, distribute_elements, expand_for_copy, expand_to_groups, expand_to_groups_among,
    flip_elements, group_patches, is_single_group, materialize_elements, reorder_elements,
    serialize_selection, ungroup_patches, AlignMode, FlipAxis, ZOrderMode,
};
pub use engine::{
    DrawEngine, EngineEvents, HoverCursor, NoopPainter, PaintView, Painter, TextEditRequest,
};
pub use export::{elements_from_json, scene_to_json, scene_to_svg, OsidrawFile};
pub use freehand::points_bounds;
pub use history::SnapshotHistory;
pub use interaction::{
    constrain_to_angle, is_degenerate_linear, is_degenerate_rect, is_linear_tool, is_shape_tool,
    linear_from_drag, rect_from_drag, snap_move, tool_for_key, Axis, DrawTool, LinearDrag,
    SnapGuide, SnapResult,
};
pub use math::{clamp, hash_string, lerp, round_px, smoothstep};
pub use render::{
    dark_theme, default_arrowhead, is_roughable, light_theme, DrawTheme, TEXT_LINE_HEIGHT,
};
pub use scene::{
    apply_style_patch, attach_point, bindable_at, bump_version, create_element,
    create_element_default, default_element_style, element_bounds, element_center,
    element_rotated_bounds, hit_test, hit_test_element, is_bindable_element, is_linear_element,
    layout_label, linear_endpoints, linear_from_endpoints, merge_style, new_element_id,
    normalize_rect, refresh_bindings, scene_bounds, Arrowhead, DrawElement, DrawElementStyle,
    DrawElementStylePatch, DrawElementType, FillStyle, Geometry, Rect, Scene, StrokeStyle,
    ARROWHEADS, BINDING_GAP, LABEL_PADDING,
};
pub use selection::{
    elements_in_marquee, handle_local_point, hit_handle, marquee_rect, resize_element,
    rotate_element, rotate_point, selection_corners, selection_corners_padded,
    selection_handle_points, selection_handles, HandleKind, HandleLayout, HandlePoint,
    RESIZE_HANDLES,
};
