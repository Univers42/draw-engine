//! Benchmarks for elbow arrows: routing one, and the per-frame cost of the gestures that
//! route one on every pointer move — drawing its head across a board, and dragging a
//! shape it is bound to.
//!
//! `draw/frame_on_*` is the one to watch: every frame routes the arrow and finds the
//! shape under its end, so a cost that tracks the size of the board shows up there.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;

use draw_engine::engine::ArrowType;
use draw_engine::scene::elbow::{self, Board};
use draw_engine::scene::element::{create_element_default, DrawElement, DrawElementType, Geometry};
use draw_engine::{DrawEngine, DrawTool, Scene};

fn shape(x: f64, y: f64) -> DrawElement {
    let mut element = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x,
            y,
            width: 40.0,
            height: 30.0,
        },
    );
    element.background_color = "#ffec99".into();
    element
}

/// `n` shapes in a grid, deterministic so a run measures the code and not the input.
fn board_of(n: usize) -> Vec<DrawElement> {
    (0..n)
        .map(|i| shape((i % 40) as f64 * 60.0, (i / 40) as f64 * 60.0))
        .collect()
}

fn elbow_between(a: &DrawElement, b: &DrawElement, board: &Board) -> DrawElement {
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: a.x + a.width,
            y: a.y,
            width: 0.0,
            height: 0.0,
        },
    );
    arrow.elbowed = Some(true);
    arrow.roundness = None;
    arrow.points = Some(vec![
        [0.0, 0.0],
        [b.x - a.x - a.width, b.y + b.height - a.y],
    ]);
    elbow::bind_and_route(&mut arrow, a, b, board, 1.0);
    arrow
}

fn route(c: &mut Criterion) {
    let mut group = c.benchmark_group("route");
    let (a, b) = (shape(0.0, 0.0), shape(300.0, 200.0));
    let pair = [a.clone(), b.clone()];
    let board = Board::new(pair.iter());
    group.bench_function("bind_and_route", |bench| {
        bench.iter(|| black_box(elbow_between(&a, &b, &board)))
    });
    let arrow = elbow_between(&a, &b, &board);
    group.bench_function("reroute", |bench| {
        bench.iter_batched_ref(
            || arrow.clone(),
            |arrow| elbow::reroute(black_box(arrow), &board),
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

/// One pointer move of an elbow arrow's head being drawn across a board of `n` shapes.
fn draw(c: &mut Criterion) {
    let mut group = c.benchmark_group("draw");
    for n in [100usize, 1000, 4000] {
        group.bench_function(format!("frame_on_{n}"), |bench| {
            bench.iter_batched_ref(
                || {
                    let mut engine = DrawEngine::new();
                    engine.set_viewport(1280.0, 800.0, 1.0);
                    engine.set_scene(Scene::new(board_of(n)));
                    engine.set_arrow_type(ArrowType::Elbow);
                    engine.set_tool(DrawTool::Arrow);
                    engine.begin_pointer(20.0, 15.0, false, false);
                    engine.move_pointer(200.0, 150.0, false, false);
                    engine
                },
                |engine| engine.move_pointer(black_box(260.0), black_box(195.0), false, false),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

/// One pointer move of a shape with `k` elbow arrows bound to it.
fn follow(c: &mut Criterion) {
    let mut group = c.benchmark_group("follow");
    for k in [1usize, 20] {
        group.bench_function(format!("shape_with_{k}_arrows"), |bench| {
            bench.iter_batched_ref(
                || {
                    let hub = shape(600.0, 400.0);
                    let spokes: Vec<DrawElement> = (0..k)
                        .map(|i| shape((i % 10) as f64 * 120.0, (i / 10) as f64 * 700.0))
                        .collect();
                    let mut elements = vec![hub.clone()];
                    elements.extend(spokes.iter().cloned());
                    let arrows: Vec<DrawElement> = {
                        let board = Board::new(elements.iter());
                        spokes
                            .iter()
                            .map(|spoke| elbow_between(spoke, &hub, &board))
                            .collect()
                    };
                    elements.extend(arrows);
                    let mut engine = DrawEngine::new();
                    engine.set_viewport(1280.0, 800.0, 1.0);
                    engine.set_scene(Scene::new(elements));
                    engine.select(vec![hub.id.clone()]);
                    engine.begin_pointer(620.0, 415.0, false, false);
                    engine.move_pointer(640.0, 425.0, false, false);
                    engine
                },
                |engine| engine.move_pointer(black_box(660.0), black_box(440.0), false, false),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(benches, route, draw, follow);
criterion_main!(benches);
