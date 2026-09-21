//! What rough geometry actually costs.
//!
//! These numbers are the budget every other decision is measured against: the shape
//! cache exists to avoid paying them per frame, and the culling exists to avoid paying
//! the replay cost for shapes nobody can see. A fill is one to two orders of magnitude
//! more expensive than an outline, which is why "hachure a large rectangle" is the case
//! that decides whether a scene stays at 60fps.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use draw_rough::{generator, FillStyle, Options};

fn opts(fill_style: FillStyle, filled: bool) -> Options {
    Options {
        seed: 12_345,
        fill_style,
        filled,
        ..Default::default()
    }
}

fn bench_outlines(c: &mut Criterion) {
    let mut group = c.benchmark_group("outline");

    group.bench_function("rectangle", |b| {
        b.iter(|| {
            black_box(generator::rectangle(
                0.0,
                0.0,
                120.0,
                80.0,
                opts(FillStyle::Hachure, false),
            ))
        })
    });

    group.bench_function("ellipse", |b| {
        b.iter(|| {
            black_box(generator::ellipse(
                60.0,
                40.0,
                120.0,
                80.0,
                opts(FillStyle::Hachure, false),
            ))
        })
    });

    let diamond = [[60.0, 0.0], [120.0, 40.0], [60.0, 80.0], [0.0, 40.0]];
    group.bench_function("diamond", |b| {
        b.iter(|| {
            black_box(generator::polygon(
                &diamond,
                opts(FillStyle::Hachure, false),
            ))
        })
    });

    let path = [[0.0, 0.0], [40.0, 30.0], [80.0, 0.0], [120.0, 30.0]];
    group.bench_function("linear_path", |b| {
        b.iter(|| {
            black_box(generator::linear_path(
                &path,
                opts(FillStyle::Hachure, false),
            ))
        })
    });

    group.finish();
}

fn bench_fills(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill");

    // Small and large, because a hachure's cost scales with area over gap — a big
    // rectangle is not a little more expensive than a small one, it is very much more.
    for (label, w, h) in [("120x80", 120.0, 80.0), ("1200x800", 1200.0, 800.0)] {
        for (name, style) in [
            ("hachure", FillStyle::Hachure),
            ("cross_hatch", FillStyle::CrossHatch),
            ("zigzag", FillStyle::ZigZag),
            ("solid", FillStyle::Solid),
        ] {
            group.bench_function(format!("{name}/{label}"), |b| {
                b.iter(|| black_box(generator::rectangle(0.0, 0.0, w, h, opts(style, true))))
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_outlines, bench_fills);
criterion_main!(benches);
