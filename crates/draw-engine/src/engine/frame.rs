use crate::camera::{Camera, WorldBounds};
use crate::engine::DrawEngine;
use crate::interaction::SnapGuide;
use crate::render::{DrawTheme, GridSettings};
use crate::scene::DrawElement;
use crate::selection::marquee_rect;

/// Everything a painter needs for one frame.
///
/// Elements are **borrowed from the scene**, not cloned. This used to own
/// `Vec<DrawElement>`, so every frame deep-copied the entire scene — three `String`s and
/// a point vector per element — whether anything had changed or not. Borrowing makes a
/// frame's setup cost proportional to the number of *visible* elements rather than to
/// the size of the document.
pub struct PaintView<'a> {
    /// The scene itself, for the painter's one question the rest cannot answer: what
    /// changed since the picture it holds — see [`crate::scene::Scene::changes_since`].
    pub scene: &'a crate::scene::Scene,
    pub camera: Camera,
    pub theme: DrawTheme,
    pub grid: GridSettings,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    /// Device pixels per world unit **at rest** — the camera scale times the quality
    /// device-pixel ratio. What geometry is simplified for.
    ///
    /// Not the scale actually being drawn at, which drops while things move: simplifying
    /// for that would rebuild every cached path when motion starts and again when it
    /// stops, which is exactly the hitch simplifying is meant to prevent.
    pub detail_scale: f64,
    pub in_motion: bool,
    /// Visible elements in z-order. Already culled to the viewport.
    pub elements: Vec<&'a DrawElement>,
    /// A number that changes whenever the scene does.
    ///
    /// The painter reuses its offscreen layer between frames and needs to know whether
    /// the picture is still the one in it. Deliberately taken from the whole scene rather
    /// than from the culled list above: panning changes which elements are visible, and a
    /// signal that moved with the camera would throw the layer away exactly when it was
    /// most worth keeping.
    pub scene_revision: u64,
    /// The elements the gesture in progress changes each frame, drawn live over a cached
    /// picture of everything else. Empty between gestures. See `engine/live.rs`.
    pub live: &'a std::collections::HashSet<String>,
    /// Moves only when something *outside* `live` changes: what the cached picture of
    /// everything else is keyed on while a gesture runs.
    pub static_revision: u64,
    /// What the eraser sweep in progress has marked. Painted at a fifth of its opacity,
    /// and so is everything a marked frame holds.
    pub erasing: &'a std::collections::HashSet<String>,
    /// Changes whenever `erasing` does. The painter keys its cached layer on it.
    pub erasing_revision: u64,
    pub selected: Vec<&'a DrawElement>,
    pub marquee: Option<WorldBounds>,
    /// The lasso loop in progress, in world space. Empty unless one is being drawn.
    pub lasso: Vec<crate::camera::Point>,
    /// Clip boxes for children that stick out of the frame that owns them, by element id.
    ///
    /// Only the ones that need it. A child sitting wholly inside its frame has nothing to
    /// clip, and setting a clip path for it anyway costs a path per element every frame
    /// for no visible difference.
    pub frame_clips: std::collections::HashMap<String, WorldBounds>,
    /// Where each frame's name sits, and what it says.
    /// `(anchor, name, frame id)` — the id so a name fades with a frame the eraser has
    /// marked.
    pub frame_names: Vec<(crate::camera::Point, String, String)>,
    /// Laser strokes to fill, oldest first, in world space.
    ///
    /// Already shaped: each is a closed outline whose width varies along its length, not
    /// a centre line to be stroked. The engine computes them so that every host draws the
    /// same beam, and so that none of them has to own the fade.
    pub laser: Vec<Vec<crate::interaction::LaserPoint>>,
    /// The colour laser strokes are filled with.
    pub laser_color: String,
    /// Each peer's laser strokes, with the colour they are filled with: theirs.
    pub peer_lasers: Vec<(String, Vec<Vec<crate::interaction::LaserPoint>>)>,
    pub snap_guides: Vec<SnapGuide>,
    pub rotate_gap: f64,
    pub handle_px: f64,
    /// Where the frame and handles sit. Shared with the hit test so the painter can only
    /// ever draw handles that are actually grabbable, and vice versa.
    pub handle_layout: crate::selection::HandleLayout,
    /// The shape a dragged arrow endpoint would bind to. Painted as a halo on its
    /// outline, so the attachment is visible before it is committed.
    pub binding_highlight: Option<&'a DrawElement>,
    /// Point handles for a selected line or arrow, **or** for one being placed.
    ///
    /// Non-empty when exactly one linear element is selected, and while a path is being
    /// built point by point. In the selected case the frame and box handles are
    /// suppressed entirely — a linear element is edited by its points, and Excalidraw
    /// shows no bounding box for one either.
    pub linear_handles: Vec<crate::selection::LinearHandlePoint>,
    /// The handle the pointer is currently moving, if any.
    ///
    /// Painted in the focus colour rather than the resting one, so that during a drag it
    /// is obvious *which* point is being moved. On a path whose points are a few pixels
    /// apart that is otherwise guesswork, and letting go of the wrong one is a bend in
    /// the wrong place.
    pub active_handle: Option<crate::selection::LinearHandle>,
    /// Corner-radius handles in world space, clockwise from the top-left; empty unless
    /// exactly one rectangle is selected and it is large enough on screen to hold them.
    pub radius_handles: Vec<crate::camera::Point>,
    /// The radius handle being dragged, painted in the focus colour.
    pub active_radius_handle: Option<usize>,
    /// Who else is here and what they hold: outlined in their colour, with their name.
    pub peer_marks: Vec<PeerMark<'a>>,
}

/// One peer's hold, as the painter draws it. See `peers.rs`.
pub struct PeerMark<'a> {
    pub name: &'a str,
    pub color: &'a str,
    /// What they hold, as it is shown — their preview of it, when they have one.
    pub elements: Vec<&'a DrawElement>,
}

pub trait Painter {
    fn paint(&mut self, view: &PaintView<'_>);
}

pub struct NoopPainter;

impl Painter for NoopPainter {
    fn paint(&mut self, _view: &PaintView<'_>) {}
}

impl DrawEngine {
    pub fn take_dirty(&mut self) -> bool {
        let dirty = self.dirty;
        self.dirty = false;
        dirty
    }

    pub fn paint_view(&self) -> PaintView<'_> {
        let selected: Vec<&DrawElement> = self
            .selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| !el.is_deleted)
            .collect();
        let lasso = match &self.interaction {
            Some(super::Interaction::Lasso { path, .. }) => path.clone(),
            _ => Vec::new(),
        };
        let marquee = match &self.interaction {
            Some(super::Interaction::Marquee { start, current, .. }) => {
                Some(marquee_rect(start.x, start.y, current.x, current.y))
            }
            _ => None,
        };
        let dpr = if self.in_motion() {
            self.interactive_dpr()
        } else {
            self.quality_dpr()
        };
        let visible = crate::camera::visible_world_rect(self.camera, self.width, self.height);
        let scene_revision = self.scene.revision();

        // Computed before the struct literal takes ownership of `selected`.
        let min_segment = super::LINEAR_MIDPOINT_MIN_PX / self.camera.scale;
        let linear_handles = match self.multi_linear.as_ref() {
            // A path being placed shows a joint on every point it has taken, so the
            // articulation of what is being drawn is visible while it is being drawn —
            // without them a polyline is an anonymous run of segments and there is no
            // way to see where the corners you placed actually landed.
            //
            // Committed points only: the last one follows the cursor and a circle riding
            // under the pointer would obscure exactly the spot being aimed at. Midpoints
            // are left out for the same reason — they add a point when dragged, which is
            // not a thing to offer on a path that is still growing.
            Some(state) => self
                .scene
                .get(&state.id)
                .map(|element| {
                    let mut handles = crate::selection::linear::handle_points(element, f64::MAX);
                    handles.truncate(state.committed);
                    handles
                })
                .unwrap_or_default(),
            None => match selected.as_slice() {
                [single] if self.shows_point_handles(single) => {
                    crate::selection::linear::handle_points(single, min_segment)
                }
                _ => Vec::new(),
            },
        };
        let active_handle = match &self.interaction {
            Some(super::Interaction::LinearPoint { handle, .. }) => Some(*handle),
            _ => None,
        };
        let radius_handles = self.radius_handles();
        let active_radius_handle = match &self.interaction {
            Some(super::Interaction::CornerRadius { corner, .. }) => Some(*corner),
            _ => None,
        };
        // Frame chrome, decided here so every host paints the same boundaries and clips
        // the same children. A host is handed boxes and labels, not rules.
        let mut frame_clips = std::collections::HashMap::new();
        let mut frame_names = Vec::new();
        for frame in self.scene.iter_ordered().filter(|el| {
            crate::scene::is_frame(el)
                && !el.is_deleted
                && crate::render::bounds::intersects_viewport(el, &visible)
        }) {
            if let Some(name) = frame.name.clone() {
                frame_names.push((
                    crate::scene::frame_name_anchor(frame),
                    name,
                    frame.id.clone(),
                ));
            }
            let clip = crate::scene::frame_clip_bounds(frame);
            for child_id in crate::scene::frame_children(self.scene.iter_ordered(), &frame.id) {
                if let Some(child) = self.scene.get(&child_id) {
                    if crate::scene::needs_frame_clip(child, frame) {
                        frame_clips.insert(child_id, clip);
                    }
                }
            }
        }

        // What peers are doing right now, painted in place of what is committed: a shape
        // moves on every screen while it is being moved. See `peers.rs`.
        let previews = self.previews();
        let mut elements: Vec<&DrawElement> = self
            .scene
            .iter_ordered()
            .map(|element| {
                previews
                    .get(element.id.as_str())
                    .copied()
                    .unwrap_or(element)
            })
            .filter(|element| {
                !element.is_deleted && crate::render::bounds::intersects_viewport(element, &visible)
            })
            .collect();
        // What a peer is drawing that is not in the scene yet goes on top, where it will be.
        for peer in self.peers() {
            for element in &peer.preview {
                if self.scene.get(&element.id).is_none()
                    && !element.is_deleted
                    && crate::render::bounds::intersects_viewport(element, &visible)
                {
                    elements.push(element);
                }
            }
        }
        let peer_marks = self
            .peers()
            .iter()
            .filter_map(|peer| {
                let held: Vec<&DrawElement> = peer
                    .holds
                    .iter()
                    .chain(peer.preview.iter().map(|element| &element.id))
                    .filter_map(|id| {
                        previews
                            .get(id.as_str())
                            .copied()
                            .or_else(|| self.scene.get(id))
                    })
                    .filter(|element| !element.is_deleted)
                    .collect();
                (!held.is_empty()).then_some(PeerMark {
                    name: &peer.name,
                    color: &peer.color,
                    elements: held,
                })
            })
            .collect();

        PaintView {
            scene: &self.scene,
            camera: self.camera,
            theme: self.theme.clone(),
            grid: self.grid,
            width: self.width,
            height: self.height,
            dpr,
            detail_scale: self.camera.scale * self.quality_dpr(),
            in_motion: self.in_motion(),
            // Culled here rather than in the painter: an element off-screen costs a
            // bounds check instead of a full path replay, which is what keeps a large
            // document responsive when you are zoomed in on one corner of it.
            elements,
            scene_revision,
            live: self.scene.live(),
            static_revision: self.scene.static_revision(),
            erasing: &self.erasing,
            erasing_revision: self.erasing_revision,
            selected,
            marquee,
            lasso,
            frame_clips,
            frame_names,
            laser: self.laser.outlines(self.now_ms, self.camera.scale),
            laser_color: crate::interaction::DEFAULT_LASER_COLOR.to_string(),
            peer_lasers: self.peer_laser_outlines(),
            snap_guides: self.snap_guides.clone(),
            rotate_gap: super::ROTATE_GAP_PX / self.camera.scale,
            handle_px: super::HANDLE_PX,
            handle_layout: self.handle_layout(),
            binding_highlight: self
                .binding_highlight
                .as_deref()
                .and_then(|id| self.scene.get(id)),
            linear_handles,
            active_handle,
            radius_handles,
            active_radius_handle,
            peer_marks,
        }
    }

    pub fn paint_if_dirty(&mut self, painter: &mut impl Painter) {
        if self.disposed || !self.dirty {
            return;
        }
        self.dirty = false;
        painter.paint(&self.paint_view());
        if self.needs_frame() {
            self.dirty = true;
        }
    }
}
