//! Benchmarks for text layout: laying out a board full of labels, and the frame that
//! draws them.
//!
//! - `text_layout/font_size_1k_labels` — one font-size change with 1,000 labelled shapes
//!   selected: every label re-wrapped from its source, measured, placed, and its shape
//!   grown where it no longer fits. One call, so it is the whole relayout path.
//! - `text_layout/fonts_loaded_1k_labels` — the same 1,000 labels laid out again when a
//!   font finishes loading (no history, no stamps).
//! - `paint_view/texts_1k` — building the frame for a view with 1,000 labelled shapes on
//!   screen: what the engine hands the painter every frame.
//! - `resize_label/east_20_moves_2k` / `south_20_moves_2k` — twenty pointer moves of a
//!   drag on a 400 × 300 shape's east or south handle, its label 2,000 characters long:
//!   the label laid out again on every move (`handleBindTextResize`). The east drag
//!   changes the width the label wraps at on every move; the south one never does, so
//!   its lines come out of the wrap memo.
//!
//! Measured with the host estimate (`default_measure`), so these time the engine's own
//! work; in the browser every uncached measure is also a `measureText` call.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;

use draw_engine::camera::Camera;
use draw_engine::scene::element::{create_element_default, DrawElement, DrawElementType, Geometry};
use draw_engine::{DrawEngine, Scene};

const LABEL: &str = "the quick brown fox jumps over the lazy dog";

/// `n` rectangles in a grid, each with a label long enough to wrap, in `family`.
fn labelled_board(n: usize, family: Option<u8>) -> Vec<DrawElement> {
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let row = (i / 40) as f64;
        let col = (i % 40) as f64;
        let mut shape = create_element_default(
            DrawElementType::Rectangle,
            Geometry {
                x: col * 220.0,
                y: row * 160.0,
                width: 180.0,
                height: 120.0,
            },
        );
        shape.id = format!("shape-{i}");
        let mut label = create_element_default(
            DrawElementType::Text,
            Geometry {
                x: shape.x + 8.0,
                y: shape.y + 35.0,
                width: 164.0,
                height: 50.0,
            },
        );
        label.id = format!("label-{i}");
        label.text = Some(LABEL.into());
        label.font_size = Some(20.0);
        label.font_family = family;
        label.container_id = Some(shape.id.clone());
        shape.bound_text_id = Some(label.id.clone());
        out.push(shape);
        out.push(label);
    }
    out
}

fn engine_of(n: usize, family: Option<u8>) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(1440.0, 900.0, 1.0);
    engine.set_scene(Scene::new(labelled_board(n, family)));
    engine
}

fn text_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_layout");
    group.sample_size(20);

    group.bench_function("font_size_1k_labels", |b| {
        b.iter_batched_ref(
            || {
                let mut engine = engine_of(1000, None);
                let shapes = (0..1000).map(|i| format!("shape-{i}")).collect();
                engine.select(shapes);
                engine
            },
            |engine| {
                engine.set_font_size(28.0);
                black_box(engine.get_font_size())
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_function("fonts_loaded_1k_labels", |b| {
        b.iter_batched_ref(
            || engine_of(1000, Some(5)),
            |engine| {
                engine.fonts_loaded();
                black_box(engine.needs_frame())
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

fn paint_view(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_view");
    group.bench_function("texts_1k", |b| {
        let mut engine = engine_of(1000, None);
        // Everything on screen: 40 × 25 shapes at 220 × 160 fit 1440 × 900 at 0.16.
        engine.camera = Camera {
            x: 0.0,
            y: 0.0,
            scale: 0.16,
        };
        b.iter(|| {
            let view = engine.paint_view();
            black_box(view.elements.len())
        });
    });
    group.finish();
}

/// A 400 × 300 shape at the origin with a 2,000-character label, selected, and the
/// pointer pressed on its `handle` (east or south), eight pixels out where it is drawn.
fn pressed_on_labelled_shape(east: bool) -> DrawEngine {
    let mut shape = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        },
    );
    shape.id = "shape".into();
    let mut label = create_element_default(
        DrawElementType::Text,
        Geometry {
            x: 5.0,
            y: 5.0,
            width: 390.0,
            height: 290.0,
        },
    );
    label.id = "label".into();
    let source = format!("{LABEL} ").repeat(46);
    label.text = Some(source[..2000].to_owned());
    label.font_size = Some(20.0);
    label.container_id = Some(shape.id.clone());
    shape.bound_text_id = Some(label.id.clone());
    let mut engine = DrawEngine::new();
    engine.set_viewport(1440.0, 900.0, 1.0);
    engine.set_scene(Scene::new(vec![shape, label]));
    engine.select(vec!["shape".into()]);
    if east {
        engine.begin_pointer(408.0, 150.0, false, false);
    } else {
        engine.begin_pointer(200.0, 308.0, false, false);
    }
    engine
}

fn resize_label(c: &mut Criterion) {
    let mut group = c.benchmark_group("resize_label");
    group.bench_function("east_20_moves_2k", |b| {
        b.iter_batched_ref(
            || pressed_on_labelled_shape(true),
            |engine| {
                for step in 1..=20 {
                    engine.move_pointer(408.0 - f64::from(step) * 5.0, 150.0, false, false);
                }
                black_box(engine.needs_frame())
            },
            BatchSize::LargeInput,
        );
    });
    group.bench_function("south_20_moves_2k", |b| {
        b.iter_batched_ref(
            || pressed_on_labelled_shape(false),
            |engine| {
                for step in 1..=20 {
                    engine.move_pointer(200.0, 308.0 + f64::from(step) * 5.0, false, false);
                }
                black_box(engine.needs_frame())
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(benches, text_layout, paint_view, resize_label);
criterion_main!(benches);
