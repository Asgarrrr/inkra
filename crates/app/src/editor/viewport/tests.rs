use proptest::prelude::*;

use super::*;

const LINE: f32 = 28.;
const VIEWPORT: Range<f32> = 0. ..280.;

fn viewport() -> Range<Pixels> {
    px(VIEWPORT.start)..px(VIEWPORT.end)
}

fn line(n: f32) -> Range<Pixels> {
    px(n * LINE)..px((n + 1.) * LINE)
}

fn top_line(offset: usize) -> TopLine {
    TopLine {
        offset: ByteOffset(offset),
        below: px(0.),
    }
}

fn top(offset: usize) -> ScrollTo {
    ScrollTo::Top(top_line(offset))
}

#[test]
fn a_reveal_before_the_first_paint_waits_for_it() {
    let mut viewport = Viewport::default();

    viewport.reveal(None, ByteOffset(0), Reveal::Center);

    assert_eq!(
        viewport.take_pending(),
        Some(ScrollTo::Cursor(Reveal::Center))
    );
    assert_eq!(viewport.handle().offset().y, px(0.));
}

#[test]
fn a_reveal_replaces_a_waiting_top_line() {
    let mut viewport = Viewport::default();

    viewport.request(top(5));
    viewport.request(ScrollTo::Cursor(Reveal::Minimal));

    assert_eq!(
        viewport.take_pending(),
        Some(ScrollTo::Cursor(Reveal::Minimal))
    );
}

#[test]
fn a_top_line_never_replaces_a_waiting_reveal() {
    let mut viewport = Viewport::default();

    viewport.request(ScrollTo::Cursor(Reveal::Center));
    viewport.request(top(5));

    assert_eq!(
        viewport.take_pending(),
        Some(ScrollTo::Cursor(Reveal::Center))
    );
}

#[test]
fn a_capture_reads_a_waiting_top_line() {
    let mut viewport = Viewport::default();
    viewport.request(top(50));

    assert_eq!(viewport.capture(None, ByteOffset(7)), top_line(50));
}

#[test]
fn a_capture_behind_a_waiting_reveal_puts_the_caret_on_top() {
    let mut viewport = Viewport::default();
    viewport.request(ScrollTo::Cursor(Reveal::Center));

    assert_eq!(viewport.capture(None, ByteOffset(7)), top_line(7));
}

#[test]
fn an_unscrolled_capture_is_the_note_top() {
    assert_eq!(
        Viewport::default().capture(None, ByteOffset(7)),
        TopLine::default()
    );
}

/// Inserts a ten-byte line at 0: an offset past it moves by ten.
fn ten_bytes_at_the_start() -> Diff {
    let old = "line\n".repeat(20);
    inkra_text::diff(0, &old, &format!("ten bytes\n{old}"))
}

#[test]
fn a_reload_maps_a_waiting_top_line_through_the_diff() {
    let mut viewport = Viewport::default();
    viewport.request(top(50));

    viewport.reloaded(&ten_bytes_at_the_start(), None);

    assert_eq!(viewport.take_pending(), Some(top(60)));
}

#[test]
fn a_reload_keeps_a_waiting_reveal() {
    let mut viewport = Viewport::default();
    viewport.request(ScrollTo::Cursor(Reveal::Center));

    viewport.reloaded(&ten_bytes_at_the_start(), None);

    assert_eq!(
        viewport.take_pending(),
        Some(ScrollTo::Cursor(Reveal::Center))
    );
}

/// Its fresh top line was read from a layout of the text before the first reload.
#[test]
fn a_second_reload_drops_its_top_line_for_the_waiting_one() {
    let mut viewport = Viewport::default();
    viewport.request(top(50));

    viewport.follow(&ten_bytes_at_the_start(), Some(top_line(20)));

    assert_eq!(viewport.take_pending(), Some(top(60)));
}

#[test]
fn a_first_reload_maps_its_own_top_line() {
    let mut viewport = Viewport::default();

    viewport.follow(&ten_bytes_at_the_start(), Some(top_line(50)));

    assert_eq!(viewport.take_pending(), Some(top(60)));
}

/// A top line on "one", nine pixels into its row, parked before `old` becomes `new`.
fn parked_through(old: &str, new: &str) -> (TopLine, TopLine) {
    let top = TopLine {
        offset: ByteOffset(5),
        below: px(9.),
    };
    let mut buffer = Buffer::new(old);
    let parked = top.park(&mut buffer);
    let diff = inkra_text::diff(buffer.version(), old, new);
    assert!(buffer.apply_diff(std::time::Instant::now(), &diff));
    (parked.unpark(&mut buffer), top.mapped(&diff))
}

#[test]
fn a_parked_top_line_follows_a_diff_as_mapped_does() {
    let old = "zero\none\ntwo\n";
    for new in ["new\nzero\none\ntwo\n", "zero\nONE\ntwo\n"] {
        let (unparked, mapped) = parked_through(old, new);
        assert_eq!(unparked, mapped, "{new:?}");
    }

    let (unparked, mapped) = parked_through(old, "zero\ntwo\n");
    assert_eq!(unparked, mapped);
    assert_eq!(unparked.offset(), ByteOffset(5), "the deleted row's start");
    assert_eq!(unparked.below(), px(9.));
}

#[test]
#[should_panic(expected = "not live")]
fn a_parked_top_line_forgets_its_anchor() {
    let mut buffer = Buffer::new("one\ntwo\n");
    let parked = TopLine::at(ByteOffset(4)).park(&mut buffer);
    let anchor = parked.offset;
    let _ = parked.unpark(&mut buffer);
    let _ = buffer.resolve(anchor);
}

#[test]
fn a_visible_line_does_not_scroll() {
    assert_eq!(minimal(line(5.), viewport(), px(0.), px(LINE)), None);
}

#[test]
fn a_line_above_the_viewport_comes_down_to_the_margin() {
    let offset = px(-20. * LINE);
    let revealed = minimal(line(10.), viewport(), offset, px(LINE)).unwrap();
    assert_eq!(revealed, px(-(10. - 3.) * LINE));
}

#[test]
fn a_line_below_the_viewport_comes_up_to_the_margin() {
    let revealed = minimal(line(20.), viewport(), px(0.), px(LINE)).unwrap();
    assert_eq!(revealed, px(VIEWPORT.end - 3. * LINE - 21. * LINE));
}

#[test]
fn a_short_viewport_clamps_the_margin_instead_of_honouring_it() {
    let short = px(0.)..px(4. * LINE);
    let revealed = minimal(line(20.), short.clone(), px(0.), px(LINE)).unwrap();

    // (4 - 1) / 2 = 1.5 lines of margin, not the 3 asked for.
    assert_eq!(revealed, px(4. * LINE - 1.5 * LINE - 21. * LINE));
    let top = px(20. * LINE) + revealed;
    assert!(top >= short.start && top + px(LINE) <= short.end);
}

#[test]
fn a_viewport_shorter_than_a_line_settles_instead_of_oscillating() {
    let cramped = px(0.)..px(10.);
    let first = minimal(line(20.), cramped.clone(), px(0.), px(LINE)).unwrap();
    assert_eq!(minimal(line(20.), cramped, first, px(LINE)), None);
}

#[test]
fn a_viewport_exactly_one_line_tall_has_one_valid_offset() {
    let exact = px(0.)..px(LINE);
    let revealed = minimal(line(20.), exact.clone(), px(0.), px(LINE)).unwrap();
    assert_eq!(revealed, px(-20. * LINE));
    assert_eq!(minimal(line(20.), exact, revealed, px(LINE)), None);
}

#[test]
fn revealing_near_the_start_stops_at_the_top() {
    let revealed = minimal(line(1.), viewport(), px(-10. * LINE), px(LINE)).unwrap();
    assert_eq!(revealed, px(0.));
}

proptest! {
    #[test]
    fn revealing_settles_and_never_scrolls_past_the_start(
        line_top in 0f32..10_000.,
        row_height in 1f32..200.,
        margin_line_height in 1f32..200.,
        viewport_top in 0f32..2_000.,
        viewport_height in 0f32..2_000.,
        offset in -10_000f32..=0.,
    ) {
        let line = px(line_top)..px(line_top + row_height);
        let viewport = px(viewport_top)..px(viewport_top + viewport_height);

        let margin = px(margin_line_height);
        let revealed = minimal(line.clone(), viewport.clone(), px(offset), margin);
        let Some(revealed) = revealed else {
            return Ok(());
        };
        prop_assert!(revealed <= px(0.), "scrolled above the document start: {revealed:?}");
        prop_assert_eq!(minimal(line, viewport, revealed, margin), None, "did not settle");
    }
}
