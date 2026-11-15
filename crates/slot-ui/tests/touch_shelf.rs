//! Touch on the carousel (Siham 2026-11-16): the carts follow the finger,
//! a fling coasts and settles on a cart, a tap finds the cart under it.
use slot_store::{Cart, Platform};
use slot_ui::{out_w, Shelf};

fn shelf_with(n: usize) -> Shelf {
    Shelf::new(
        (0..n)
            .map(|i| Cart {
                platform: Platform::Gba,
                stem: format!("Game {i}"),
                rom: format!("Games/GBA/Game {i}.gba").into(),
                label: None,
                code: String::new(),
                shell: None, battery: false,
                title: format!("GAME {i}"),
            })
            .collect(),
    )
}

fn run(s: &mut Shelf, seconds: f32) {
    for _ in 0..(seconds * 120.0) as usize { s.update(1.0 / 120.0); }
}

#[test]
fn dragging_left_brings_the_next_carts_and_stays_under_the_finger() {
    let mut s = shelf_with(10);
    run(&mut s, 1.0);
    s.grab();
    let before = s.scroll;
    s.drag_by(-30.0);
    assert!(s.scroll > before, "finger left shows carts on the right");
    let moved = s.scroll;
    run(&mut s, 0.5);
    assert_eq!(s.scroll, moved, "nothing moves the carts but the finger");
    assert!(s.is_scrolling(), "the carousel keeps drawing while held");
}

#[test]
fn dragging_past_half_a_cart_selects_the_next_one() {
    let mut s = shelf_with(10);
    run(&mut s, 1.0);
    s.grab();
    for _ in 0..40 { s.drag_by(-10.0); }
    assert!(s.index >= 1, "index {}", s.index);
    s.let_go(0.0);
    run(&mut s, 2.0);
    assert!((s.scroll - s.scroll.round()).abs() < 0.01, "settles on a cart: {}", s.scroll);
    assert!(!s.is_scrolling());
}

#[test]
fn a_fling_coasts_over_several_carts_then_settles_on_one() {
    let mut s = shelf_with(30);
    run(&mut s, 1.0);
    s.grab();
    s.drag_by(-20.0);
    s.let_go(-3000.0);
    run(&mut s, 4.0);
    assert!(s.index >= 3, "a fling goes far: {}", s.index);
    assert!(!s.is_scrolling(), "and comes to rest");
    assert!((s.scroll - s.scroll.round()).abs() < 0.01, "on a cart: {}", s.scroll);
}

#[test]
fn a_slow_release_settles_without_coasting() {
    let mut s = shelf_with(10);
    run(&mut s, 1.0);
    s.grab();
    s.drag_by(-5.0);
    s.let_go(-50.0);
    run(&mut s, 2.0);
    assert_eq!(s.index, 0);
}

#[test]
fn the_dpad_takes_over_from_a_fling() {
    let mut s = shelf_with(20);
    run(&mut s, 1.0);
    s.grab();
    s.let_go(-4000.0);
    s.update(1.0 / 120.0);
    s.hold_right(0);
    s.release_right();
    let index = s.index;
    run(&mut s, 2.0);
    assert_eq!(s.index, index, "the fling stops at the D-pad's cart");
}

#[test]
fn a_tap_finds_the_cart_under_it() {
    let mut s = shelf_with(10);
    run(&mut s, 1.0);
    let mid = out_w() as f32 / 2.0;
    let y = (0..480).map(|y| y as f32).find(|&y| s.cart_under(mid, y) == Some(0));
    let y = y.expect("the middle cart can be tapped");
    assert_eq!(s.cart_under(mid, y), Some(0));
    assert_eq!(s.cart_under(mid, 2.0), None, "the top of the screen is not a cart");
    let right = (0..(out_w() as usize / 2)).map(|d| mid + d as f32)
        .find(|&x| s.cart_under(x, y) == Some(1));
    assert!(right.is_some(), "the cart on the right can be tapped");
}

#[test]
fn a_lone_cart_only_gives_a_little() {
    let mut s = shelf_with(1);
    run(&mut s, 1.0);
    s.grab();
    for _ in 0..50 { s.drag_by(-20.0); }
    assert!((s.scroll - s.scroll_target()).abs() <= 0.3 + 1e-4);
    s.let_go(-2000.0);
    run(&mut s, 2.0);
    assert!(!s.is_scrolling());
    assert_eq!(s.index, 0);
}
