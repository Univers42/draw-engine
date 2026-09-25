//! Benchmarks for the editing loop: duplicate, move, erase.
//!
//! Each one measures a gesture a person actually repeats, on a board that is already
//! large — which is the case that matters, because every cost here that is proportional
//! to the size of the *document* rather than to the size of the *edit* turns a fast
//! editor into one that gets slower the more you use it.
//!
//! `duplicate/run_of_100` is the one to watch. Holding Ctrl+D is how a board full of the
//! same shape gets made, and each press used to cost a deep clone of the whole scene plus
//! a JSON round trip — so the run was quadratic in its own output and the hundredth press
//! cost a hundred times the first.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;

use draw_engine::render::cache::ShapeCache;
use draw_engine::scene::element::{create_element_default, DrawElement, DrawElementType, Geometry};
use draw_engine::{DrawEngine, DrawTool, Scene, ZOrderMode};

/// `n` shapes in a grid, deterministic so a run measures the code and not the input.
fn board_of(n: usize) -> Vec<DrawElement> {
    (0..n)
        .map(|i| {
            let row = (i / 40) as f64;
            let col = (i % 40) as f64;
            let mut element = create_element_default(
                DrawElementType::Rectangle,
                Geometry {
                    x: col * 60.0,
                    y: row * 60.0,
                    width: 40.0,
                    height: 30.0,
                },
            );
            element.background_color = "#ffec99".into();
            element
        })
        .collect()
}

fn engine_of(n: usize) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(1280.0, 800.0, 1.0);
    engine.set_scene(Scene::new(board_of(n)));
    engine
}

fn duplicate(c: &mut Criterion) {
    let mut group = c.benchmark_group("duplicate");

    // One press on an already-large board. Should cost the size of the *selection*, not
    // the size of the board — if this tracks `n`, something is walking the whole scene.
    for n in [100usize, 1000, 4000] {
        group.bench_function(format!("one_of_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut engine = engine_of(n);
                    let first = engine.get_scene()[0].id.clone();
                    engine.select(vec![first]);
                    engine
                },
                |engine| {
                    engine.duplicate_selection(12.0, 12.0);
                    black_box(engine.get_scene().len())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // The gesture as it is actually performed: hold Ctrl+D. Each press duplicates what
    // the last one produced, so the board doubles — and any per-press cost proportional
    // to the board compounds.
    group.bench_function("run_of_100", |b| {
        b.iter_batched_ref(
            || {
                let mut engine = engine_of(1);
                let first = engine.get_scene()[0].id.clone();
                engine.select(vec![first]);
                engine
            },
            |engine| {
                for _ in 0..100 {
                    engine.duplicate_selection(2.0, 2.0);
                }
                black_box(engine.get_scene().len())
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn erase(c: &mut Criterion) {
    let mut group = c.benchmark_group("erase");

    // One sweep across a board, the way the eraser is used. The sweep is a handful of
    // pointer samples, because the host coalesces moves to one per frame.
    for n in [100usize, 1000] {
        group.bench_function(format!("sweep_over_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut engine = engine_of(n);
                    engine.set_tool(DrawTool::Eraser);
                    engine
                },
                |engine| {
                    engine.begin_pointer(0.0, 0.0, false, false);
                    for step in 1..=20 {
                        let t = f64::from(step) / 20.0;
                        engine.move_pointer(t * 1200.0, t * 700.0, false, false);
                    }
                    engine.end_pointer();
                    black_box(engine.get_scene().len())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // A stack of identical shapes under one point — what a run of Ctrl+D leaves behind.
    // One pass of the eraser has to take all of them, so this measures the whole job
    // rather than one layer of it.
    group.bench_function("stack_of_300", |b| {
        b.iter_batched_ref(
            || {
                let mut engine = DrawEngine::new();
                engine.set_viewport(1280.0, 800.0, 1.0);
                let mut stacked = Vec::new();
                for _ in 0..300 {
                    let mut element = create_element_default(
                        DrawElementType::Rectangle,
                        Geometry {
                            x: 100.0,
                            y: 100.0,
                            width: 200.0,
                            height: 150.0,
                        },
                    );
                    element.background_color = "#ffec99".into();
                    stacked.push(element);
                }
                engine.set_scene(Scene::new(stacked));
                engine.set_tool(DrawTool::Eraser);
                engine
            },
            |engine| {
                engine.begin_pointer(200.0, 175.0, false, false);
                engine.end_pointer();
                black_box(engine.get_scene().iter().filter(|e| !e.is_deleted).count())
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn moving(c: &mut Criterion) {
    let mut group = c.benchmark_group("move");

    // Picking a shape up. Once per drag, and unavoidably proportional to the board: it
    // is where the snap candidates are gathered and where the group is expanded.
    for n in [100usize, 1000, 4000] {
        group.bench_function(format!("pick_up_one_of_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut engine = engine_of(n);
                    engine.set_tool(DrawTool::Select);
                    engine
                },
                |engine| {
                    engine.begin_pointer(20.0, 15.0, false, false);
                    black_box(engine.get_selection().len())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // The frames of the drag, with the pick-up outside the measurement. This is the
    // number that decides whether a drag keeps up with the cursor: 20 moves is 20 frames,
    // so it has to stay well inside 20 × 16ms — and, more to the point, it should not
    // grow with the board at all. Everything it does per frame is supposed to be
    // proportional to the selection and to what is on screen, not to the document.
    for n in [100usize, 1000, 4000] {
        group.bench_function(format!("frames_of_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut engine = engine_of(n);
                    engine.set_tool(DrawTool::Select);
                    engine.begin_pointer(20.0, 15.0, false, false);
                    engine
                },
                |engine| {
                    for step in 1..=20 {
                        engine.move_pointer(20.0 + f64::from(step) * 5.0, 15.0, false, false);
                    }
                    black_box(engine.get_selection().len())
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn reordering(c: &mut Criterion) {
    let mut group = c.benchmark_group("reorder");

    // A z-order command as a person issues it, the whole of it: the restacking, the step
    // of undo, and what the host is handed — a delta carrying just the order, not the
    // whole scene (`Scene::reorder_live`). Each should grow with the board and no faster
    // — the oracle rebuilds the array once per run of the selection, and `every_other`
    // cuts the selection into n/2 runs. 20k added alongside `frame_join`/`remote_patch`'s
    // top size to measure the delta-vs-full-scene win at the same board size.
    for n in [1000usize, 5000, 20000] {
        group.bench_function(format!("one_to_front_of_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut engine = engine_of(n);
                    let first = engine.get_scene()[0].id.clone();
                    engine.select(vec![first]);
                    engine
                },
                |engine| {
                    engine.reorder_selection(ZOrderMode::Front);
                    black_box(engine.get_selection().len())
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function(format!("every_other_forward_of_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut engine = engine_of(n);
                    let ids = engine.get_scene().into_iter().step_by(2).map(|el| el.id);
                    engine.select(ids.collect());
                    engine
                },
                |engine| {
                    engine.reorder_selection(ZOrderMode::Forward);
                    black_box(engine.get_selection().len())
                },
                BatchSize::SmallInput,
            );
        });
        // The same command on a board that has deleted as much as it holds. Tombstones stay
        // in the scene, and `Scene::set_order` walked the new order once per tombstone.
        group.bench_function(format!("one_to_front_of_{n}_over_{n}_deleted"), |b| {
            b.iter_batched_ref(
                || {
                    let mut elements = board_of(2 * n);
                    for element in elements.iter_mut().skip(1).step_by(2) {
                        element.is_deleted = true;
                    }
                    let first = elements[0].id.clone();
                    let mut engine = DrawEngine::new();
                    engine.set_viewport(1280.0, 800.0, 1.0);
                    engine.set_scene(Scene::new(elements));
                    engine.select(vec![first]);
                    engine
                },
                |engine| {
                    engine.reorder_selection(ZOrderMode::Front);
                    black_box(engine.get_selection().len())
                },
                BatchSize::SmallInput,
            );
        });
        // A child of each of n/50 frames brought to the front of its own frame: one pass
        // over the stack per frame, as the oracle's.
        group.bench_function(format!("a_child_of_each_frame_to_front_of_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut elements = board_of(n);
                    let mut firsts = Vec::new();
                    for block in elements.chunks_mut(50) {
                        let (frame, children) = block.split_last_mut().expect("50 a block");
                        frame.kind = DrawElementType::Frame;
                        for child in children.iter_mut() {
                            child.frame_id = Some(frame.id.clone());
                        }
                        firsts.push(children[0].id.clone());
                    }
                    let mut engine = DrawEngine::new();
                    engine.set_viewport(1280.0, 800.0, 1.0);
                    engine.set_scene(Scene::new(elements));
                    engine.select(firsts);
                    engine
                },
                |engine| {
                    engine.reorder_selection(ZOrderMode::Front);
                    black_box(engine.get_selection().len())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // What actually leaves the wasm boundary: an order-only delta now
    // (`Scene::reorder_live`), the whole scene serialised before the fix in
    // `followups.md`'s "U". The restack above costs the same either way — this is the
    // part the fix changes. 5k and 20k, as the wave-4 brief asks.
    for n in [5000usize, 20000] {
        group.bench_function(format!("delta_of_{n}"), |b| {
            b.iter_batched_ref(
                || {
                    let mut engine = engine_of(n);
                    let first = engine.get_scene()[0].id.clone();
                    engine.select(vec![first]);
                    let _ = engine.drain_events();
                    engine
                },
                |engine| {
                    engine.reorder_selection(ZOrderMode::Front);
                    black_box(engine.drain_events())
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// The properties panel's read of the selection, asked once per `style_revision`.
///
/// Proportional to the selection, and paid once per change rather than once per row or
/// per frame — so the number to watch is the whole board selected. `json` adds what the
/// wasm boundary costs on top: the host receives it serialised. `one_of_n` is the other
/// end, where the group check for a single element walks the board. `in_pairs` is the
/// whole board grouped two by two, so every align unit is found by its group.
fn selection_style(c: &mut Criterion) {
    let mut group = c.benchmark_group("selection_style");
    for n in [1000usize, 5000] {
        let mut engine = engine_of(n);
        engine.select_all();
        group.bench_function(format!("all_of_{n}"), |b| {
            b.iter(|| black_box(engine.selection_style()));
        });
        group.bench_function(format!("all_of_{n}_json"), |b| {
            b.iter(|| black_box(serde_json::to_string(&engine.selection_style()).unwrap()));
        });
        let first = engine.get_scene()[0].id.clone();
        engine.select(vec![first]);
        group.bench_function(format!("one_of_{n}"), |b| {
            b.iter(|| black_box(engine.selection_style()));
        });

        let mut paired = board_of(n);
        for (i, element) in paired.iter_mut().enumerate() {
            element.group_ids = vec![format!("pair-{}", i / 2)];
        }
        engine.set_scene(Scene::new(paired));
        engine.select_all();
        group.bench_function(format!("all_of_{n}_in_pairs"), |b| {
            b.iter(|| black_box(engine.selection_style()));
        });
    }
    group.finish();
}

/// One rectangle drawn, as a person draws it — press, drag, release, and what the host
/// is handed — inside a frame and, for comparison, outside it. Inside, the shape goes
/// directly below the frame (`stack_under_frame`), which moves it in the stack: that
/// should cost the board's ids, not the board.
fn joining(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_join");
    group.sample_size(20);
    for n in [1000usize, 5000, 20000] {
        let setup = move || {
            let mut elements = board_of(n);
            for element in &mut elements {
                element.x += 10_000.0;
            }
            let mut frame = create_element_default(
                DrawElementType::Frame,
                Geometry {
                    x: 0.0,
                    y: 0.0,
                    width: 600.0,
                    height: 400.0,
                },
            );
            frame.name = Some("Frame 1".into());
            let mut child = create_element_default(
                DrawElementType::Rectangle,
                Geometry {
                    x: 20.0,
                    y: 20.0,
                    width: 40.0,
                    height: 40.0,
                },
            );
            child.frame_id = Some(frame.id.clone());
            elements.insert(0, child);
            elements.push(frame);
            let mut engine = DrawEngine::new();
            engine.set_viewport(1280.0, 800.0, 1.0);
            engine.set_scene(Scene::new(elements));
            let _ = engine.drain_events();
            engine.set_tool(DrawTool::Rectangle);
            engine
        };
        for (name, x) in [("inside", 100.0), ("outside", 700.0)] {
            group.bench_function(format!("draw_{name}_of_{n}"), |b| {
                b.iter_batched_ref(
                    setup,
                    |engine| {
                        engine.begin_pointer(x, 100.0, false, false);
                        engine.move_pointer(x + 40.0, 130.0, false, false);
                        engine.end_pointer();
                        black_box(engine.drain_events())
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

/// A peer's patch that moved the stack, as every shape a peer draws into a frame sends:
/// the order of every live id, one of them moved. Should cost the board, once.
fn remote(c: &mut Criterion) {
    let mut group = c.benchmark_group("remote_patch");
    group.sample_size(20);
    for n in [1000usize, 5000, 20000] {
        let setup = move || {
            let mut engine = engine_of(n);
            let mut order: Vec<String> = engine.get_scene().into_iter().map(|el| el.id).collect();
            let moved = order.remove(0);
            order.insert(n - 1, moved);
            let patch = serde_json::json!({
                "type": "osidraw",
                "version": 1,
                "elements": [],
                "order": order,
            })
            .to_string();
            let _ = engine.drain_events();
            (engine, patch)
        };
        group.bench_function(format!("order_of_{n}"), |b| {
            b.iter_batched_ref(
                setup,
                |(engine, patch)| black_box(engine.apply_remote_patch(patch)),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    duplicate,
    erase,
    joining,
    moving,
    painting,
    remote,
    reordering,
    selection_style
);
criterion_main!(benches);

/// Screen-sized shapes, hundreds of them, each a copy of the last.
///
/// The case that actually hurts, and the one the small-shape benchmarks above miss
/// entirely. Rough geometry costs grow with a shape's *size* — a hachure fill of a
/// 1200×700 rectangle is thousands of ops where a 40×30 one is a handful — and culling
/// rejects nothing, because every one of these covers the viewport.
fn big_duplicates(n: usize) -> Vec<DrawElement> {
    let mut first = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 700.0,
        },
    );
    first.background_color = "#ffec99".into();
    first.seed = 123_456;
    (0..n)
        .map(|i| {
            // What Ctrl+D produces: a fresh id, the same seed, the same size, nudged.
            let mut copy = first.clone();
            copy.id = format!("copy-{i}");
            copy.x += f64::from(i as u32) * 12.0;
            copy.y += f64::from(i as u32) * 12.0;
            copy
        })
        .collect()
}

fn painting(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_big");
    group.sample_size(20);

    // The first frame after a run of Ctrl+D: nothing is cached yet. Every copy is
    // identical in element-local space — same seed, same size, same style — so this is
    // the same geometry generated over and over.
    for n in [50usize, 200] {
        group.bench_function(format!("cold_cache_{n}_copies"), |b| {
            b.iter_batched_ref(
                || (ShapeCache::new(), big_duplicates(n)),
                |(cache, elements)| {
                    for element in elements.iter() {
                        black_box(cache.get(element));
                    }
                    black_box(cache.len())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // Every frame after that. Should be a lookup per element and nothing else.
    group.bench_function("warm_cache_200_copies", |b| {
        b.iter_batched_ref(
            || {
                let elements = big_duplicates(200);
                let mut cache = ShapeCache::new();
                for element in &elements {
                    cache.get(element);
                }
                (cache, elements)
            },
            |(cache, elements)| {
                for element in elements.iter() {
                    black_box(cache.get(element));
                }
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}
