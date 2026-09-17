//! GPUI's custom Element API keeps the visible buffer in one layout node. Text
//! is shaped by GPUI, with Helix's grapheme columns preserved for hit testing.
use crate::view::EditorView;
use gpui::{prelude::*, *};
use helix_view::graphics::{Color, CursorKind, Modifier, UnderlineStyle as HelixUnderline};

pub fn color(value: Color, fallback: u32) -> Hsla {
    const ANSI: [u32; 16] = [
        0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5, 0x666666,
        0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
    ];
    let indexed = |i: u8| -> u32 {
        match i {
            0..=15 => ANSI[i as usize],
            16..=231 => {
                let v = i as u32 - 16;
                let component = |n| if n == 0 { 0 } else { 55 + n * 40 };
                (component(v / 36) << 16) | (component(v / 6 % 6) << 8) | component(v % 6)
            }
            _ => {
                let v = 8 + 10 * (i as u32 - 232);
                (v << 16) | (v << 8) | v
            }
        }
    };
    rgb(match value {
        Color::Reset => fallback,
        Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
        Color::Indexed(i) => indexed(i),
        Color::Black => ANSI[0],
        Color::Red => ANSI[1],
        Color::Green => ANSI[2],
        Color::Yellow => ANSI[3],
        Color::Blue => ANSI[4],
        Color::Magenta => ANSI[5],
        Color::Cyan => ANSI[6],
        Color::Gray => ANSI[8],
        Color::LightGray => ANSI[7],
        Color::LightRed => ANSI[9],
        Color::LightGreen => ANSI[10],
        Color::LightYellow => ANSI[11],
        Color::LightBlue => ANSI[12],
        Color::LightMagenta => ANSI[13],
        Color::LightCyan => ANSI[14],
        Color::White => ANSI[15],
    })
    .into()
}

pub struct BufferElement {
    pub view: Entity<EditorView>,
    pub preview: Option<std::sync::Arc<tui::buffer::Buffer>>,
}
impl IntoElement for BufferElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for BufferElement {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.preview.is_none() {
            self.view
                .update(cx, |view, cx| view.layout(bounds, window, cx));
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.view.read(cx);
        if view.trace_latency {
            if let Some(input_at) = view.frame.input_at {
                if view.last_presented_input.replace(Some(input_at)) != Some(input_at) {
                    log::info!(
                        "GPUI input_to_paint_ms={:.2}",
                        input_at.elapsed().as_secs_f64() * 1000.
                    );
                }
            }
        }
        let frame = view.frame.clone();
        let buffer = self.preview.as_deref().unwrap_or(&frame.buffer);
        let focus = view.focus.clone();
        let cell_width = view.cell_width;
        let line_height = view.line_height;
        let font_size = view.font_size;
        let base_font = view.font.clone();
        let preedit = view.preedit.clone();
        if self.preview.is_none() {
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.view.clone()),
                cx,
            );
        }
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, color(frame.background, 0x1b1d27)));
            for (y, row) in buffer
                .content
                .chunks(buffer.area.width.max(1) as usize)
                .enumerate()
            {
                if line_height * y as f32 >= bounds.size.height {
                    break;
                }
                let mut x = 0;
                while x < row.len() {
                    let cell = &row[x];
                    let start = x;
                    let style = cell.style();
                    let mut text = String::new();
                    // Shape contiguous runs, never one element per cell. Wide graphemes
                    // get their own run so their following placeholder isn't drawn.
                    let wide = cell.width() > 1;
                    while x < row.len()
                        && row[x].style() == style
                        && (x == start || (!wide && row[x].width() == 1))
                    {
                        text.push_str(&row[x].symbol);
                        x += row[x].width().max(1);
                        if wide {
                            break;
                        }
                    }
                    let mut fg = color(cell.fg, 0xd8dee9);
                    let mut bg = color(cell.bg, 0x1b1d27);
                    if cell.modifier.contains(Modifier::REVERSED) {
                        std::mem::swap(&mut fg, &mut bg);
                    }
                    if cell.modifier.contains(Modifier::DIM) {
                        fg.a *= 0.6;
                    }
                    let origin =
                        bounds.origin + point(cell_width * start as f32, line_height * y as f32);
                    window.paint_quad(fill(
                        Bounds::new(origin, size(cell_width * (x - start) as f32, line_height)),
                        bg,
                    ));
                    let mut font = base_font.clone();
                    if cell.modifier.contains(Modifier::BOLD) {
                        font.weight = FontWeight::BOLD;
                    }
                    if cell.modifier.contains(Modifier::ITALIC) {
                        font.style = FontStyle::Italic;
                    }
                    let run = TextRun {
                        len: text.len(),
                        font,
                        color: fg,
                        background_color: None,
                        underline: (cell.underline_style != HelixUnderline::Reset).then(|| {
                            UnderlineStyle {
                                color: Some(color(cell.underline_color, 0xd8dee9)),
                                thickness: px(1.),
                                wavy: cell.underline_style == HelixUnderline::Curl,
                            }
                        }),
                        strikethrough: cell.modifier.contains(Modifier::CROSSED_OUT).then_some(
                            StrikethroughStyle {
                                color: Some(fg),
                                thickness: px(1.),
                            },
                        ),
                    };
                    if !cell.modifier.contains(Modifier::HIDDEN)
                        && !text.chars().all(|ch| ch == ' ')
                    {
                        let line = window.text_system().shape_line(
                            text.into(),
                            font_size,
                            &[run],
                            Some(cell_width),
                        );
                        if let Err(error) = line.paint(origin, line_height, window, cx) {
                            log::error!("Text paint failed: {error}");
                        }
                    }
                }
            }
            if self.preview.is_some() {
                return;
            }
            let origin = bounds.origin
                + point(
                    cell_width * frame.cursor.0 as f32,
                    line_height * frame.cursor.1 as f32,
                );
            let cursor_bounds = match frame.cursor_kind {
                CursorKind::Bar => Bounds::new(origin, size(px(2.), line_height)),
                CursorKind::Underline => Bounds::new(
                    origin + point(px(0.), line_height - px(2.)),
                    size(cell_width, px(2.)),
                ),
                _ => Bounds::new(origin, size(cell_width, line_height)),
            };
            if frame.cursor_kind != CursorKind::Hidden {
                window.paint_quad(fill(cursor_bounds, rgba(0xe5e9f066)));
            }
            if !preedit.is_empty() && !frame.widgets.iter().any(|widget| widget.has_input()) {
                let run = TextRun {
                    len: preedit.len(),
                    font: base_font.clone(),
                    color: rgb(0xffffff).into(),
                    background_color: Some(rgb(0x303446).into()),
                    underline: Some(UnderlineStyle {
                        color: None,
                        thickness: px(1.),
                        wavy: false,
                    }),
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(
                    preedit.into(),
                    font_size,
                    &[run],
                    Some(cell_width),
                );
                let _ = line.paint(origin, line_height, window, cx);
            }
        });
    }
}
