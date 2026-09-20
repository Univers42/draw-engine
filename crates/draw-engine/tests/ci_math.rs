mod common;
use common::*;
use draw_engine::*;

#[test]
fn clamp_within_bounds() {
    assert_close(clamp(5.0, 0.0, 10.0), 5.0);
}

#[test]
fn clamp_below_min() {
    assert_close(clamp(-5.0, 0.0, 10.0), 0.0);
}

#[test]
fn clamp_above_max() {
    assert_close(clamp(15.0, 0.0, 10.0), 10.0);
}

#[test]
fn clamp_at_exact_boundaries() {
    assert_close(clamp(0.0, 0.0, 10.0), 0.0);
    assert_close(clamp(10.0, 0.0, 10.0), 10.0);
}

#[test]
fn lerp_at_zero() {
    assert_close(lerp(10.0, 20.0, 0.0), 10.0);
}

#[test]
fn lerp_at_one() {
    assert_close(lerp(10.0, 20.0, 1.0), 20.0);
}

#[test]
fn lerp_at_half() {
    assert_close(lerp(10.0, 20.0, 0.5), 15.0);
}

#[test]
fn lerp_extrapolation_negative() {
    assert_close(lerp(10.0, 20.0, -1.0), 0.0);
}

#[test]
fn lerp_extrapolation_beyond_one() {
    assert_close(lerp(10.0, 20.0, 2.0), 30.0);
}

#[test]
fn smoothstep_zero() {
    assert_close(smoothstep(0.0), 0.0);
}

#[test]
fn smoothstep_one() {
    assert_close(smoothstep(1.0), 1.0);
}

#[test]
fn smoothstep_midpoint() {
    assert_close(smoothstep(0.5), 0.5);
}

#[test]
fn smoothstep_clamped_below_zero() {
    assert_close(smoothstep(-2.0), 0.0);
}

#[test]
fn smoothstep_clamped_above_one() {
    assert_close(smoothstep(3.0), 1.0);
}

#[test]
fn hash_string_empty_is_zero() {
    assert_eq!(hash_string(""), 0);
}

#[test]
fn hash_string_deterministic_ascii() {
    let h1 = hash_string("hello world");
    let h2 = hash_string("hello world");
    assert_eq!(h1, h2);
    assert_ne!(h1, hash_string("hello world!"));
}

#[test]
fn hash_string_unicode_support() {
    let h = hash_string("こんにちは世界");
    assert_ne!(h, 0);
}

#[test]
fn round_px_behavior() {
    assert_close(round_px(4.2), 4.0);
    assert_close(round_px(4.8), 5.0);
    assert_close(round_px(-3.7), -4.0);
}

#[test]
fn points_bounds_empty_slice() {
    assert_eq!(points_bounds(&[]), [0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn points_bounds_single_point() {
    assert_eq!(points_bounds(&[[12.0, 34.0]]), [12.0, 34.0, 12.0, 34.0]);
}

#[test]
fn points_bounds_multiple_points() {
    let pts = [[10.0, 20.0], [-5.0, 50.0], [30.0, -15.0]];
    assert_eq!(points_bounds(&pts), [-5.0, -15.0, 30.0, 50.0]);
}
