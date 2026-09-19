use gpui::{
    App, Application, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    rgb, size,
};
use gpui_test_core::Buffer;

struct EditorView {
    buffer: Buffer,
}

impl Render for EditorView {
    // RRGGBB hex literals are the idiomatic gpui color form; clippy's suggested
    // underscore grouping (e.g. 0x001e_1e1e) obscures the RGB byte boundaries.
    #[allow(clippy::unreadable_literal)]
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size(px(500.0))
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .p_4()
            .child(self.buffer.text().to_string())
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.0), px(500.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| EditorView {
                    buffer: Buffer::new("gpui-test scaffold — core wired to app."),
                })
            },
        );
        if let Err(err) = result {
            eprintln!("failed to open window: {err}");
            std::process::exit(1);
        }
    });
}
