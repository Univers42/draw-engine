//! Track B: the command palette's "Add rectangle / diamond / ellipse" — a keyboard/palette
//! route to a new shape that never drags a pointer. Excalidraw's own command palette offers
//! no shape-insertion action at `@1118751f` to port, so the default size below is this
//! project's own pick: roughly a flowchart node's proportions, immediately useful once
//! Ctrl/Cmd+Arrow (`flowchart.rs`) grows a connected diagram off it.

use crate::engine::DrawEngine;
use crate::scene::{create_element, DrawElementType, Geometry};

/// A new rectangle/diamond/ellipse's width and height, centred on the insertion point.
const DEFAULT_SHAPE_SIZE: (f64, f64) = (120.0, 60.0);

impl DrawEngine {
    /// Inserts a default-sized rectangle/diamond/ellipse/figure centred at `(sx, sy)`, in
    /// the current default style ([`DrawEngine::get_next_style`] — the same style a
    /// hand-drawn shape would start with), selected, as one step of history —
    /// `paste_json`'s own shape (`clipboard.rs`). `None` for any other kind, with nothing
    /// added. A figure takes whatever the Shapes picker has queued (`next_figure`), same
    /// as one dragged out with the pointer.
    ///
    /// `sx`/`sy` are **screen** coordinates, like every other pointer-driven entry point
    /// (`insert_image`, `insert_embed` — `image.rs`): a toolbar/palette insert reports the
    /// middle of the viewport, which is a screen rectangle, not a world one.
    pub fn insert_default_shape(
        &mut self,
        kind: DrawElementType,
        sx: f64,
        sy: f64,
    ) -> Option<String> {
        if !matches!(
            kind,
            DrawElementType::Rectangle
                | DrawElementType::Diamond
                | DrawElementType::Ellipse
                | DrawElementType::Figure
        ) {
            return None;
        }
        let centre = self.screen_to_world(sx, sy);
        let (width, height) = DEFAULT_SHAPE_SIZE;
        let mut element = create_element(
            kind,
            Geometry {
                x: centre.x - width / 2.0,
                y: centre.y - height / 2.0,
                width,
                height,
            },
            self.get_next_style(),
            self.now_ms,
        );
        if kind == DrawElementType::Figure {
            element.figure = Some(self.next_figure.clone());
        }
        let id = element.id.clone();
        self.scene.add(element);
        self.set_selection(vec![id.clone()]);
        self.push_history();
        self.request_draw();
        Some(id)
    }
}
