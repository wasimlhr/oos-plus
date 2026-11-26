// SPDX-License-Identifier: GPL-2.0-or-later

//! The time cursors in Overlay, Stacked and Windows: the line, the tag that is pressed and the tag that is drawn belong
//! to the window of the cursor's source.

use super::*;

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    u.step(0.05);
}

fn setup(layout: Layout, a: windows::Arrangement, mode: CursorMode) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = layout;
    u.studio.enter = layout == Layout::Studio;
    for c in 0..NCH {
        u.m.ch[c].on = true;
    }
    for _ in 0..10 {
        u.step(0.05);
    }
    u.m.run = RunState::Stop;
    u.set_arrangement(a);
    u.m.cursor = mode;
    (u.m.cur_a, u.m.cur_b) = (0.25, 0.75);
    assert!(u.set_cursor_source(WaveId::Ch(2)));
    if mode == CursorMode::Track {
        assert!(u.set_cursor_b_source(WaveId::Ch(2)));
    }
    for _ in 0..3 {
        frame(&mut u);
    }
    u
}

/// Where the first and second time cursor are: a fraction of the window of the cursor's source.
fn line_x(u: &Ui, frac: f32) -> i32 {
    let w = u.source_area(WaveId::Ch(2)).expect("the source has a window");
    w.x + (frac * w.w as f32) as i32
}

#[test]
fn a_cursor_tag_stands_on_the_line_in_every_arrangement() {
    for layout in [Layout::Default, Layout::Studio] {
        for a in [windows::Arrangement::Overlay, windows::Arrangement::Stacked, windows::Arrangement::Windows] {
            for mode in [CursorMode::Time, CursorMode::Track] {
                let u = setup(layout, a, mode);
                let tags = u.cursor_tags();
                assert!(tags.len() >= 2, "{layout:?} {a:?} {mode:?}: tags {tags:?}");
                for (k, frac) in [(0usize, 0.25f32), (1, 0.75)] {
                    let want = line_x(&u, frac);
                    // Default puts the tag just right of its line, Studio centres it on the line
                    let hits = tags
                        .iter()
                        .filter(|(kk, r)| {
                            *kk == k
                                && (want - 4..=want + 4)
                                    .contains(&(if layout == Layout::Studio { r.x + r.w / 2 } else { r.x - 2 }))
                        })
                        .count();
                    assert!(hits >= 1, "{layout:?} {a:?} {mode:?}: no tag for cursor {k} at x {want}; tags {tags:?}");
                }
            }
        }
    }
}

fn touch(u: &mut Ui, phase: Phase, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase, x, y });
    u.step(0.01);
}

/// A finger on the line of a time cursor drags it by the pixels it moves over the width of the window of its source.
#[test]
fn a_cursor_line_drags_by_the_pixels_over_its_window() {
    for layout in [Layout::Default, Layout::Studio] {
        for a in [windows::Arrangement::Overlay, windows::Arrangement::Stacked, windows::Arrangement::Windows] {
            let mut u = setup(layout, a, CursorMode::Time);
            let w = u.source_area(WaveId::Ch(2)).unwrap();
            let (x, y) = (line_x(&u, 0.25), w.y + w.h / 2);
            touch(&mut u, Phase::Down, x, y);
            for s in 1..=6 {
                touch(&mut u, Phase::Move, x + 5 * s, y);
            }
            touch(&mut u, Phase::Up, x + 30, y);
            frame(&mut u);
            let want = 0.25 + 30.0 / w.w as f32;
            assert!(
                (u.m.cur_a - want).abs() < 0.01,
                "{layout:?} {a:?}: cursor A at {} want {want} (window {w:?})",
                u.m.cur_a
            );
            assert!((u.m.cur_b - 0.75).abs() < 1e-6, "{layout:?} {a:?}: cursor B stays");
        }
    }
}

/// In Windows the cursors stay in the window of their source: moving them changes nothing in the other windows.
#[test]
fn the_cursors_stay_in_their_window_in_windows() {
    for layout in [Layout::Default, Layout::Studio] {
        let mut u = setup(layout, windows::Arrangement::Windows, CursorMode::Time);
        let mut a = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut a);
        (u.m.cur_a, u.m.cur_b) = (0.40, 0.60);
        let mut b = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut b);
        let wins = u.plot.wins.clone();
        assert_eq!(wins.len(), 4, "{layout:?}: four windows");
        let own = u.ch_area(2);
        let diff = |w: &Rect| {
            let mut n = 0;
            for y in w.y + 2..w.bottom() - 2 {
                for x in w.x + 2..w.right() - 2 {
                    let i = (y * u.scr.w + x) as usize;
                    n += usize::from(a.px[i] != b.px[i]);
                }
            }
            n
        };
        assert!(diff(&own) > 0, "{layout:?}: the cursors are drawn in the window of their source");
        for w in wins.iter().filter(|w| **w != own) {
            assert_eq!(diff(w), 0, "{layout:?}: the cursors reach the window {w:?} of another source");
        }
    }
}

/// The readout box of the cursors stands under the tags and the value boxes of the time cursors, not over them.
#[test]
fn the_readout_box_clears_the_cursor_tags() {
    for a in [windows::Arrangement::Overlay, windows::Arrangement::Stacked, windows::Arrangement::Windows] {
        let u = setup(Layout::Studio, a, CursorMode::Track);
        let lane = u.source_area(WaveId::Ch(2)).unwrap();
        // (the tags on the top edge of the lane; the plain X1 / X2 tags at its foot are not in the way)
        for (k, r) in u.cursor_tags().into_iter().filter(|(_, r)| r.y < lane.y + 30) {
            assert!(r.bottom() <= lane.y + 48, "{a:?}: tag {k} {r:?} reaches into the box at {}", lane.y + 48);
        }
    }
}
