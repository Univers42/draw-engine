//! Benchmarks for the render path.
//!
//! These exist to keep specific regressions from coming back, so each one names the
//! thing it guards:
//!
//! - `paint_view` was deep-cloning the entire scene every frame. It now borrows and
//!   culls, so its cost should track the number of *visible* elements, not the size of
//!   the document. Compare `paint_view/zoomed_in` against `paint_view/all_visible`:
//!   if they converge, culling has stopped working.
//! - `shape_cache` guards the rule that translation is free and resizing is not.
//! - `generate` is the raw cost of rough geometry, which is what the cache is avoiding.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;

use draw_engine::camera::Camera;
use draw_engine::render::cache::ShapeCache;
use draw_engine::render::shape::element_drawable;
use draw_engine::scene::element::{
    create_element, DrawElement, DrawElementStyle, DrawElementType, Geometry,
};
use draw_engine::scene::Scene;
use draw_engine::DrawEngine;

/// A scene of `n` elements spread over a grid far larger than any viewport, so culling
/// has something to do. Seeds are deterministic: a benchmark whose input varies run to
/// run measures noise.
fn scene_of(n: usize) -> Vec<DrawElement> {
    let kinds = [
        DrawElementType::Rectangle,
        DrawElementType::Ellipse,
        DrawElementType::Diamond,
    ];
    (0..n)
        .map(|i| {
            let col = (i % 100) as f64;
            let row = (i / 100) as f64;
            let mut e = create_element(
                kinds[i % kinds.len()],
                Geometry {
                    x: col * 180.0,
                    y: row * 120.0,
                    width: 120.0,
                    height: 80.0,
                },
                DrawElementStyle::default(),
                0.0,
            );
            e.seed = (i as u32).wrapping_mul(2_654_435_761).wrapping_add(1);
            e
        })
        .collect()
}

fn engine_with(n: usize) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(1440.0, 900.0, 2.0);
    engine.set_scene(Scene::new(scene_of(n)));
    engine
}

fn bench_paint_view(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_view");

    for n in [1_000usize, 10_000, 50_000] {
        // Zoomed in: only a handful of elements are on screen, so this measures how well
        // the cull rejects. It should stay near-flat as n grows.
        group.bench_function(format!("zoomed_in/{n}"), |b| {
            let engine = engine_with(n);
            b.iter(|| {
                let view = engine.paint_view();
                black_box(view.elements.len())
            });
        });

        // Zoomed out far enough that everything is visible — the worst case, where
        // culling cannot help and the cost is genuinely proportional to n.
        group.bench_function(format!("all_visible/{n}"), |b| {
            let mut engine = engine_with(n);
            // `camera` is a public field; zooming right out is the worst case for
            // culling because every element is on screen at once.
            engine.camera = Camera {
                x: 0.0,
                y: 0.0,
                scale: 0.01,
            };
            b.iter(|| {
                let view = engine.paint_view();
                black_box(view.elements.len())
            });
        });
    }

    group.finish();
}

fn bench_shape_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("shape_cache");

    let element = scene_of(1).pop().expect("one element");

    // The hot path: every frame of a drag.
    group.bench_function("hit", |b| {
        let mut cache = ShapeCache::new();
        cache.get(&element);
        b.iter(|| black_box(cache.get(&element).is_some()));
    });

    // What a hit costs us instead of: regenerating from scratch.
    group.bench_function("miss", |b| {
        b.iter_batched(
            ShapeCache::new,
            |mut cache| black_box(cache.get(&element).is_some()),
            BatchSize::SmallInput,
        );
    });

    // Moving must hit; the whole design rests on it.
    group.bench_function("after_move", |b| {
        let mut cache = ShapeCache::new();
        let mut moved = element.clone();
        cache.get(&moved);
        b.iter(|| {
            moved.x += 1.0;
            black_box(cache.get(&moved).is_some())
        });
    });

    group.finish();
}

fn bench_generate(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate");

    let mut sharp = scene_of(1).pop().expect("one element");
    sharp.roundness = None;

    let rounded = {
        let mut e = sharp.clone();
        e.roundness = Some(8.0);
        e
    };

    let filled = {
        let mut e = sharp.clone();
        e.background_color = "#ffc9c9".into();
        e
    };

    let ellipse = {
        let mut e = sharp.clone();
        e.kind = DrawElementType::Ellipse;
        e
    };

    for (name, element) in [
        ("rect_sharp", &sharp),
        ("rect_rounded", &rounded),
        ("rect_hachure", &filled),
        ("ellipse", &ellipse),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(element_drawable(black_box(element))));
        });
    }

    group.finish();
}

/// What one frame of a drag costs.
///
/// `refresh_bindings` runs on every pointer move. It used to clone the entire scene
/// twice and write every element back, so dragging one rectangle got slower as the
/// board filled up. The in-place version should be flat in the number of *bound*
/// elements, so these numbers must not scale with `n`.
fn bench_drag(c: &mut Criterion) {
    let mut group = c.benchmark_group("drag");

    for n in [1_000usize, 10_000, 50_000] {
        group.bench_function(format!("in_place/{n}"), |b| {
            let mut scene = Scene::new(scene_of(n));
            b.iter(|| {
                draw_engine::scene::binding::refresh_bindings_in_place(&mut scene);
                black_box(scene.size())
            });
        });

        // The path this replaced, kept as a benchmark so the difference is a number
        // rather than a claim: clone the whole scene, rebuild it, write every element
        // back — once per pointer move.
        group.bench_function(format!("cloning/{n}"), |b| {
            let mut scene = Scene::new(scene_of(n));
            b.iter(|| {
                for element in draw_engine::refresh_bindings(&scene.ordered_cloned()) {
                    scene.put(element);
                }
                black_box(scene.size())
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_paint_view,
    bench_shape_cache,
    bench_generate,
    bench_drag
);
criterion_main!(benches);
