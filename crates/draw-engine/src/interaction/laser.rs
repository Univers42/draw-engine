//! The laser pointer: a trail that follows the cursor and fades behind it.
//!
//! Presentation tool, not a drawing tool. Nothing here ever reaches the scene — a laser
//! stroke has no element, no id and no history entry, and reloading the board shows none
//! of it. What it has instead is a clock: the trail thins and disappears on its own.
//!
//! Transcribed from Excalidraw's `packages/laser-pointer` and `laserTrails.ts` at the SHA
//! pinned in `scripts/oracle-sha.txt`. The whole pipeline lives here rather than in a
//! host, because all of it is geometry: a host reports where the pointer went and fills
//! the polygons it is handed back.
//!
//! The shape of a stroke is not a fixed-width line. Each captured point gets a radius
//! from [`size_mapping`], and the outline walks the points twice — once down one side,
//! once back up the other — mitring the corners as it goes. A radius of zero drops the
//! point out of the outline entirely, which is what makes the trail appear to be eaten
//! from behind while the head keeps up with the cursor.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// A point on a trail: a position, and the moment it was captured.
///
/// The third component is `pressure` in Excalidraw's types, and a laser pointer has no
/// pressure to record, so they reuse the slot for `performance.now()` — that timestamp is
/// what [`size_mapping`] reads to fade the point out. The name is kept as `r` because the
/// vector helpers below carry it through every operation, which only makes sense once you
/// see that it is a third coordinate to them and a clock to us.
///
/// It matters that `r` rides along: streamlining interpolates whole points, so a
/// smoothed point's timestamp is interpolated too, and a trail drawn quickly fades in the
/// same shape it was drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaserPoint {
    pub x: f64,
    pub y: f64,
    pub r: f64,
}

impl LaserPoint {
    pub fn new(x: f64, y: f64, r: f64) -> Self {
        Self { x, y, r }
    }
}

const UNIT_X: LaserPoint = LaserPoint {
    x: 1.0,
    y: 0.0,
    r: 0.0,
};

fn add(a: LaserPoint, b: LaserPoint) -> LaserPoint {
    LaserPoint::new(a.x + b.x, a.y + b.y, a.r + b.r)
}

fn sub(a: LaserPoint, b: LaserPoint) -> LaserPoint {
    LaserPoint::new(a.x - b.x, a.y - b.y, a.r - b.r)
}

fn smul(p: LaserPoint, s: f64) -> LaserPoint {
    LaserPoint::new(p.x * s, p.y * s, p.r * s)
}

/// Unit vector in the xy plane. `r` is left alone — it is a clock, not a coordinate.
fn norm(p: LaserPoint) -> LaserPoint {
    let len = (p.x * p.x + p.y * p.y).sqrt();
    LaserPoint::new(p.x / len, p.y / len, p.r)
}

fn rot(p: LaserPoint, rad: f64) -> LaserPoint {
    let (sin, cos) = rad.sin_cos();
    LaserPoint::new(cos * p.x - sin * p.y, sin * p.x + cos * p.y, p.r)
}

/// Linear interpolation of a whole point, timestamp included.
fn plerp(a: LaserPoint, b: LaserPoint, t: f64) -> LaserPoint {
    add(a, smul(sub(b, a), t))
}

/// The angle `p1 -> p -> p2`, signed.
fn angle(p: LaserPoint, p1: LaserPoint, p2: LaserPoint) -> f64 {
    (p2.y - p.y).atan2(p2.x - p.x) - (p1.y - p.y).atan2(p1.x - p.x)
}

/// Wrap an angle into `(-PI, PI]`.
fn norm_angle(a: f64) -> f64 {
    a.sin().atan2(a.cos())
}

fn mag(p: LaserPoint) -> f64 {
    (p.x * p.x + p.y * p.y).sqrt()
}

fn dist(a: LaserPoint, b: LaserPoint) -> f64 {
    ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt()
}

/// Total length along a polyline.
///
/// The final segment is deliberately counted twice, because Excalidraw's `runLength` does
/// and this value decides when the tail is handed over to the stable list. "correcting"
/// it would shift that handover and change the smoothing, so it is transcribed as-is.
fn run_length(ps: &[LaserPoint]) -> f64 {
    if ps.len() < 2 {
        return 0.0;
    }
    let mut len = 0.0;
    for i in 1..ps.len() {
        len += dist(ps[i - 1], ps[i]);
    }
    len + dist(ps[ps.len() - 2], ps[ps.len() - 1])
}

/// Excalidraw's `easeOut`: quartic, so the fade is slow to start and quick to finish.
pub fn ease_out(k: f64) -> f64 {
    let a = 1.0 - k;
    1.0 - a * a * a * a
}

/// Radius of the laser dot, in world units before the zoom correction.
pub const LASER_SIZE: f64 = 2.0;

/// How much each new point is pulled back towards the previous one, in `0..1`.
///
/// Excalidraw's laser overrides the package default of `0.45` with this. Raw pointer
/// samples are jittery enough that an unsmoothed trail visibly shivers.
pub const LASER_STREAMLINE: f64 = 0.4;

/// How long a point takes to fade out completely, in milliseconds.
pub const LASER_DECAY_TIME_MS: f64 = 1000.0;

/// How many points from the end of the trail are visible at all.
///
/// A count of points, not a distance — Excalidraw passes the array length as
/// `totalLength`. So the trail is bounded by how many samples the pointer produced, which
/// means a fast flick leaves a longer streak than a slow drag over the same distance.
pub const LASER_DECAY_LENGTH: f64 = 50.0;

/// How long the unstabilised tail may get before it is folded into the stable points.
pub const LASER_MAX_TAIL_LENGTH: f64 = 50.0;

/// Past this turn angle a vertex is treated as a corner and gets a rounded join.
pub const CORNER_DETECTION_MAX_ANGLE_DEG: f64 = 75.0;

/// Excalidraw's default laser colour, as a hex our painters can use directly.
pub const DEFAULT_LASER_COLOR: &str = "#ff0000";

/// Corner detection is loosened when the pointer is moving quickly.
///
/// A fast stroke samples coarsely, so genuine curves arrive as sharp-looking vertices;
/// without this they would all be mitred as corners and a quick scribble would come out
/// faceted.
fn corner_detection_variance(speed: f64) -> f64 {
    if speed > 35.0 {
        0.5
    } else {
        1.0
    }
}

/// How large the trail is at one point: the smaller of the time fade and the length fade.
///
/// * `age` — milliseconds since the point was captured.
/// * `index` / `total` — position of the point in the trail, and how many there are.
///
/// Two independent reasons for a point to vanish, and the smaller wins: it is old, or it
/// is too far back in the queue. The first is what makes a stationary laser disappear; the
/// second is what bounds the streak behind a moving one.
pub fn size_mapping(age: f64, index: f64, total: f64) -> f64 {
    let t = (1.0 - age / LASER_DECAY_TIME_MS).max(0.0);
    let l = (LASER_DECAY_LENGTH - LASER_DECAY_LENGTH.min(total - index)) / LASER_DECAY_LENGTH;
    ease_out(l).min(ease_out(t))
}

/// Tunables for one trail.
#[derive(Clone, Copy, Debug)]
pub struct LaserOptions {
    pub size: f64,
    pub streamline: f64,
    /// Whether a fully faded trail still shows a dot at the cursor.
    ///
    /// Excalidraw turns this off the moment a stroke ends, so a finished trail can
    /// disappear completely instead of leaving a dot behind.
    pub keep_head: bool,
}

impl Default for LaserOptions {
    fn default() -> Self {
        Self {
            size: LASER_SIZE,
            streamline: LASER_STREAMLINE,
            keep_head: false,
        }
    }
}

/// One stroke of the laser pointer.
///
/// Points arrive in two piles. `stable` is settled and never revisited; `tail` is the
/// recent end of the stroke, still short enough that streamlining is worth redoing. The
/// tail is folded into the stable pile once it is longer than
/// [`LASER_MAX_TAIL_LENGTH`], which keeps the per-point work constant however long the
/// stroke gets.
// No `Default`: a default stroke would have `fresh: false` and claim to hold a point it
// does not have. Build one with `LaserStroke::new`.
#[derive(Clone, Debug)]
pub struct LaserStroke {
    options: LaserOptions,
    /// Points exactly as captured, before streamlining. Kept because duplicate rejection
    /// compares against the raw position, not the smoothed one.
    original: Vec<LaserPoint>,
    stable: Vec<LaserPoint>,
    tail: Vec<LaserPoint>,
    fresh: bool,
}

impl LaserStroke {
    pub fn new(options: LaserOptions) -> Self {
        Self {
            options,
            original: Vec::new(),
            stable: Vec::new(),
            tail: Vec::new(),
            fresh: true,
        }
    }

    fn last_point(&self) -> LaserPoint {
        *self
            .tail
            .last()
            .or_else(|| self.stable.last())
            .unwrap_or(&LaserPoint {
                x: 0.0,
                y: 0.0,
                r: 0.0,
            })
    }

    /// Record where the pointer is now.
    ///
    /// A repeat of the previous position is dropped: a stationary pointer still emits
    /// events, and letting those through would fill the trail with zero-length segments
    /// whose direction vectors are undefined.
    pub fn add_point(&mut self, x: f64, y: f64, now: f64) {
        let mut point = LaserPoint::new(x, y, now);
        if let Some(last) = self.original.last() {
            if last.x == point.x && last.y == point.y {
                return;
            }
        }
        self.original.push(point);

        if self.fresh {
            self.fresh = false;
            self.stable.push(point);
            return;
        }

        if self.options.streamline > 0.0 {
            point = plerp(self.last_point(), point, 1.0 - self.options.streamline);
        }
        self.tail.push(point);

        if run_length(&self.tail) > LASER_MAX_TAIL_LENGTH {
            self.stabilize_tail();
        }
    }

    /// The pointer came up: settle the tail and stop showing a head.
    pub fn close(&mut self) {
        self.stabilize_tail();
        self.options.keep_head = false;
    }

    fn stabilize_tail(&mut self) {
        self.stable.append(&mut self.tail);
    }

    /// When the most recent point was captured, if there is one.
    fn newest(&self) -> Option<f64> {
        self.tail.last().or_else(|| self.stable.last()).map(|p| p.r)
    }

    /// Whether this stroke may still have something to show at `now`.
    ///
    /// A **conservative** answer, and deliberately so. It is never false while there is
    /// an outline to draw — the newest point is the youngest, so if any point is still
    /// within [`LASER_DECAY_TIME_MS`] this one is — but it can stay true for a few
    /// milliseconds after the last outline has gone, because the outline is governed by
    /// the second-to-last point rather than the last, and by the length fade as well as
    /// the time fade.
    ///
    /// Erring this way is the safe direction: being late costs one wasted frame, being
    /// early would drop a stroke that is still on screen. Answering exactly would mean
    /// restating the outline's own rules here, which is a second place to get them wrong.
    pub fn is_visible(&self, now: f64) -> bool {
        match self.newest() {
            Some(t) => now - t < LASER_DECAY_TIME_MS,
            None => false,
        }
    }

    fn size_at(
        &self,
        size_override: Option<f64>,
        point_time: f64,
        i: f64,
        total: f64,
        now: f64,
    ) -> f64 {
        size_override.unwrap_or(self.options.size) * size_mapping(now - point_time, i, total)
    }

    /// The filled outline of this stroke, in the same space the points were given in.
    ///
    /// `size_override` replaces the configured radius; a host passes `size / zoom` so the
    /// beam stays the same width on screen at every zoom level, which is what Excalidraw
    /// does and is why the radius is not baked into the points.
    ///
    /// The result is a closed polygon, ready to fill. An empty result means the stroke has
    /// faded out completely.
    pub fn outline(&self, now: f64, size_override: Option<f64>) -> Vec<LaserPoint> {
        if self.fresh {
            return Vec::new();
        }

        let points: Vec<LaserPoint> = self
            .stable
            .iter()
            .chain(self.tail.iter())
            .copied()
            .collect();
        let len = points.len();
        if len == 0 {
            return Vec::new();
        }
        let total = len as f64;
        let size_of = |p: LaserPoint, i: f64| self.size_at(size_override, p.r, i, total, now);

        // A single point is a dot.
        if len == 1 {
            let c = points[0];
            let size = size_of(c, 0.0);
            if size < 0.5 {
                return Vec::new();
            }
            let mut ps = Vec::new();
            let mut theta = 0.0;
            while theta <= TAU {
                ps.push(add(c, smul(rot(UNIT_X, theta), size)));
                theta += PI / 16.0;
            }
            ps.push(add(c, smul(UNIT_X, size)));
            return ps;
        }

        // Two points are a capsule: half a circle at each end.
        if len == 2 {
            let (c, n) = (points[0], points[1]);
            let (c_size, n_size) = (size_of(c, 0.0), size_of(n, 0.0));
            if c_size < 0.5 || n_size < 0.5 {
                return Vec::new();
            }
            let mut ps = Vec::new();
            let p_angle = angle(c, LaserPoint::new(c.x, c.y - 100.0, c.r), n);

            let mut theta = p_angle;
            while theta <= PI + p_angle {
                ps.push(add(c, smul(rot(UNIT_X, theta), c_size)));
                theta += PI / 16.0;
            }
            let mut theta = PI + p_angle;
            while theta <= TAU + p_angle {
                ps.push(add(n, smul(rot(UNIT_X, theta), n_size)));
                theta += PI / 16.0;
            }
            ps.push(ps[0]);
            return ps;
        }

        // Three or more: walk the interior points, accumulating one side into
        // `forward` and the other into `backward`, then join them with caps.
        let mut forward: Vec<LaserPoint> = Vec::new();
        let mut backward: Vec<LaserPoint> = Vec::new();

        let mut prev_speed = 0.0;
        let mut visible_start = 0usize;

        for i in 1..len - 1 {
            let (p, c, n) = (points[i - 1], points[i], points[i + 1]);

            // Smoothed sample-to-sample distance, which stands in for pointer speed.
            // Scoped to the iteration: Excalidraw hoists it, but only ever reads it
            // within the pass that set it, and carrying it out would suggest otherwise.
            let d = dist(p, c);
            let speed = prev_speed + (d - prev_speed) * 0.2;

            let c_size = size_of(c, i as f64);
            if c_size == 0.0 {
                // Faded out: everything up to here is gone, so the stroke now starts
                // after it. This is the trail being eaten from behind.
                visible_start = i + 1;
                continue;
            }

            let dir_pc = norm(sub(p, c));
            let dir_nc = norm(sub(n, c));
            let p1_dir_pc = rot(dir_pc, FRAC_PI_2);
            let p2_dir_pc = rot(dir_pc, -FRAC_PI_2);
            let p1_dir_nc = rot(dir_nc, FRAC_PI_2);
            let p2_dir_nc = rot(dir_nc, -FRAC_PI_2);

            let p1_pc = add(c, smul(p1_dir_pc, c_size));
            let p2_pc = add(c, smul(p2_dir_pc, c_size));
            let p1_nc = add(c, smul(p1_dir_nc, c_size));
            let p2_nc = add(c, smul(p2_dir_nc, c_size));

            // The mitre directions. When the two sides point exactly opposite each other
            // the sum is zero and has no direction, so the incoming edge is used instead.
            let ftdir = add(p1_dir_pc, p2_dir_nc);
            let btdir = add(p2_dir_pc, p1_dir_nc);
            let pa_pc = add(
                c,
                smul(
                    if mag(ftdir) == 0.0 {
                        dir_pc
                    } else {
                        norm(ftdir)
                    },
                    c_size,
                ),
            );
            let pa_nc = add(
                c,
                smul(
                    if mag(btdir) == 0.0 {
                        dir_nc
                    } else {
                        norm(btdir)
                    },
                    c_size,
                ),
            );

            let c_angle = norm_angle(angle(c, p, n));
            let d_angle =
                (CORNER_DETECTION_MAX_ANGLE_DEG / 180.0) * PI * corner_detection_variance(speed);

            if c_angle.abs() < d_angle {
                // Sharp enough to be a corner: round the outside, mitre the inside.
                let t_angle = norm_angle(PI - c_angle).abs();
                if t_angle == 0.0 {
                    continue;
                }
                let step = t_angle / 4.0;
                // A turn angle small enough that the step underflows to zero would spin
                // these loops forever. Excalidraw has the same shape and the same hazard;
                // a dropped vertex is invisible, a hung frame is not.
                if step <= 0.0 {
                    continue;
                }

                if c_angle < 0.0 {
                    backward.push(p2_pc);
                    backward.push(pa_nc);

                    let mut theta = 0.0;
                    while theta <= t_angle {
                        forward.push(add(c, rot(smul(p1_dir_pc, c_size), theta)));
                        theta += step;
                    }
                    let mut theta = t_angle;
                    while theta >= 0.0 {
                        backward.push(add(c, rot(smul(p1_dir_pc, c_size), theta)));
                        theta -= step;
                    }

                    backward.push(pa_nc);
                    backward.push(p1_nc);
                } else {
                    forward.push(p1_pc);
                    forward.push(pa_pc);

                    let mut theta = 0.0;
                    while theta <= t_angle {
                        backward.push(add(c, rot(smul(p1_dir_pc, -c_size), -theta)));
                        theta += step;
                    }
                    let mut theta = t_angle;
                    while theta >= 0.0 {
                        forward.push(add(c, rot(smul(p1_dir_pc, -c_size), -theta)));
                        theta -= step;
                    }

                    forward.push(pa_pc);
                    forward.push(p2_nc);
                }
            } else {
                forward.push(pa_pc);
                backward.push(pa_nc);
            }

            prev_speed = speed;
        }

        // Everything faded: either show a dot at the head, or nothing at all.
        if visible_start >= len - 2 {
            if self.options.keep_head {
                let c = points[len - 1];
                let mut ps = Vec::new();
                let mut theta = 0.0;
                while theta <= TAU {
                    ps.push(add(c, smul(rot(UNIT_X, theta), self.options.size)));
                    theta += PI / 16.0;
                }
                ps.push(add(c, smul(UNIT_X, self.options.size)));
                return ps;
            }
            return Vec::new();
        }

        let first = points[visible_start];
        let second = points[visible_start + 1];
        let penultimate = points[len - 2];
        let ultimate = points[len - 1];

        let ppdir_fs = rot(norm(sub(second, first)), -FRAC_PI_2);
        let ppdir_pu = rot(norm(sub(penultimate, ultimate)), FRAC_PI_2);

        let start_cap_size = size_of(first, 0.0);
        let end_cap_size = if self.options.keep_head {
            self.options.size
        } else {
            size_of(penultimate, (len - 2) as f64)
        };

        let mut start_cap: Vec<LaserPoint> = Vec::new();
        if start_cap_size > 0.1 {
            let mut theta = 0.0;
            while theta <= PI {
                start_cap.insert(0, add(first, rot(smul(ppdir_fs, start_cap_size), -theta)));
                theta += PI / 16.0;
            }
            start_cap.insert(0, add(first, smul(ppdir_fs, -start_cap_size)));
        } else {
            start_cap.push(first);
        }

        // One and a half turns, so the head is a full round cap however the last two
        // points happen to be oriented.
        let mut end_cap: Vec<LaserPoint> = Vec::new();
        let mut theta = 0.0;
        while theta <= PI * 3.0 {
            end_cap.push(add(ultimate, rot(smul(ppdir_pu, -end_cap_size), -theta)));
            theta += PI / 16.0;
        }

        let mut out = start_cap.clone();
        out.extend(forward);
        end_cap.reverse();
        out.extend(end_cap);
        backward.reverse();
        out.extend(backward);
        if let Some(&start) = start_cap.first() {
            out.push(start);
        }
        out
    }
}

/// Every laser stroke currently on screen.
///
/// There is at most one stroke being drawn, and any number still fading. Separating them
/// is what lets a second flick start before the first has disappeared.
#[derive(Clone, Debug, Default)]
pub struct LaserTrails {
    options: LaserOptions,
    current: Option<LaserStroke>,
    past: Vec<LaserStroke>,
}

impl LaserTrails {
    pub fn new(options: LaserOptions) -> Self {
        Self {
            options,
            current: None,
            past: Vec::new(),
        }
    }

    /// Begin a stroke at a point.
    pub fn start(&mut self, x: f64, y: f64, now: f64) {
        let mut stroke = LaserStroke::new(self.options);
        stroke.add_point(x, y, now);
        // A stroke already in progress is not discarded — it fades like any other.
        if let Some(previous) = self.current.replace(stroke) {
            self.past.push(previous);
        }
    }

    /// Extend the stroke in progress. A no-op if none is.
    pub fn add(&mut self, x: f64, y: f64, now: f64) {
        if let Some(stroke) = self.current.as_mut() {
            stroke.add_point(x, y, now);
        }
    }

    /// Finish the stroke in progress and leave it to fade.
    pub fn end(&mut self) {
        if let Some(mut stroke) = self.current.take() {
            stroke.close();
            self.past.push(stroke);
        }
    }

    /// Forget every trail immediately, faded or not.
    pub fn clear(&mut self) {
        self.current = None;
        self.past.clear();
    }

    /// Drop the strokes that have finished fading.
    ///
    /// Called once per frame. Without it a long session accumulates one dead stroke per
    /// flick, and every frame walks all of them to produce nothing.
    pub fn prune(&mut self, now: f64) {
        self.past.retain(|stroke| stroke.is_visible(now));
    }

    /// Whether anything is still on screen, and therefore whether the next frame is worth
    /// asking for.
    pub fn is_active(&self, now: f64) -> bool {
        self.current.is_some() || self.past.iter().any(|stroke| stroke.is_visible(now))
    }

    /// One fillable polygon per stroke, oldest first.
    ///
    /// `scale` is the camera zoom: the beam is specified in screen pixels, so its world
    /// radius is divided by the zoom to keep it a constant width on screen.
    pub fn outlines(&self, now: f64, scale: f64) -> Vec<Vec<LaserPoint>> {
        let size = self.options.size / scale;
        self.past
            .iter()
            .chain(self.current.iter())
            .map(|stroke| stroke.outline(now, Some(size)))
            .filter(|outline| !outline.is_empty())
            .collect()
    }
}
