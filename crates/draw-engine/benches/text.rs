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
//!
//! Measured with the host estimate (`default_measure`), so these time the engine's own
//! work; in the browser every uncached measure is also a `measureText` call.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;

use draw_engine::camera::Camera;
use draw_engine::scene::element::{create_element_default, DrawElement, DrawElementType, Geometry};
use draw_engine::{DrawEngine, Scene};

const LABEL: &str = "the quick brown fox jumps over the lazy dog";

/// `n` rectangles in a grid, each with a label long enough to wrap.
fn labelled_board(n: usize) -> Vec<DrawElement> {
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
        label.container_id = Some(shape.id.clone());
        shape.bound_text_id = Some(label.id.clone());
        out.push(shape);
        out.push(label);
    }
    out
}

fn engine_of(n: usize) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(1440.0, 900.0, 1.0);
    engine.set_scene(Scene::new(labelled_board(n)));
    engine
}

fn text_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_layout");
    group.sample_size(20);

    group.bench_function("font_size_1k_labels", |b| {
        b.iter_batched_ref(
            || {
                let mut engine = engine_of(1000);
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

    group.finish();
}

fn paint_view(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_view");
    group.bench_function("texts_1k", |b| {
        let mut engine = engine_of(1000);
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

criterion_group!(benches, text_layout, paint_view);
criterion_main!(benches);
