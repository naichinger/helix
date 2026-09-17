//! GPUI controls built from semantic Helix models, never terminal cells.
use crate::{surface::color, view::EditorView};
use gpui::{prelude::*, *};
use helix_term::{application::ApplicationEvent, frontend};
use helix_view::graphics::{Color, Modifier, Style as HelixStyle};
use tokio::sync::mpsc::UnboundedSender;

pub struct NativeScroll {
    signature: u64,
    requested: usize,
    handle: ScrollHandle,
}

fn scroll_handle(view: &EditorView, index: usize, widget: &frontend::Widget) -> ScrollHandle {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    match &widget.content {
        frontend::WidgetContent::Document { title, blocks } => {
            title.hash(&mut hash);
            for block in blocks {
                for run in &block.text.0 {
                    run.text.hash(&mut hash);
                }
            }
        }
        frontend::WidgetContent::Text { title, text } => {
            title.hash(&mut hash);
            for run in &text.0 {
                run.text.hash(&mut hash);
            }
        }
        _ => (),
    }
    let signature = hash.finish();
    let mut handles = view.native_scroll.borrow_mut();
    let state = handles.entry(index).or_insert_with(|| NativeScroll {
        signature,
        requested: 0,
        handle: ScrollHandle::new(),
    });
    if state.signature != signature {
        *state = NativeScroll {
            signature,
            requested: 0,
            handle: ScrollHandle::new(),
        };
    }
    if state.requested != widget.scroll {
        state.requested = widget.scroll;
        state
            .handle
            .set_offset(point(px(0.), -view.line_height * widget.scroll as f32));
    }
    state.handle.clone()
}

fn text_run(text: &str, style: HelixStyle, font: &Font) -> TextRun {
    let mut font = font.clone();
    if style.add_modifier.contains(Modifier::BOLD) {
        font.weight = FontWeight::BOLD;
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        font.style = FontStyle::Italic;
    }
    let mut foreground = color(style.fg.unwrap_or(Color::Reset), 0xd8dee9);
    let mut background = color(style.bg.unwrap_or(Color::Reset), 0x1b1d27);
    if style.add_modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut foreground, &mut background);
    }
    if style.add_modifier.contains(Modifier::DIM) {
        foreground.a *= 0.6;
    }
    if style.add_modifier.contains(Modifier::HIDDEN) {
        foreground.a = 0.;
    }
    TextRun {
        len: text.len(),
        font,
        color: foreground,
        // Surface fills belong to the native row/panel, not rectangular text runs.
        background_color: None,
        underline: style
            .underline_style
            .filter(|style| *style != helix_view::graphics::UnderlineStyle::Reset)
            .map(|underline| UnderlineStyle {
                color: style.underline_color.map(|value| color(value, 0xd8dee9)),
                thickness: px(1.),
                wavy: underline == helix_view::graphics::UnderlineStyle::Curl,
            }),
        strikethrough: style
            .add_modifier
            .contains(Modifier::CROSSED_OUT)
            .then_some(StrikethroughStyle {
                color: Some(foreground),
                thickness: px(1.),
            }),
    }
}

fn label(text: &frontend::RichText, base: HelixStyle, font: &Font) -> StyledText {
    let mut content = String::new();
    let mut runs = Vec::new();
    for run in &text.0 {
        content.push_str(&run.text);
        runs.push(text_run(&run.text, base.patch(run.style), font));
    }
    StyledText::new(content).with_runs(runs)
}

fn prompt_input(
    prompt: frontend::Prompt,
    style: HelixStyle,
    view: &EditorView,
    entity: Entity<EditorView>,
) -> impl IntoElement {
    let font = view.ui_font.clone();
    let font_size = px(13.);
    let line_height = view.line_height;
    let preedit = view.preedit.clone();
    let palette = crate::design::Palette::for_background(view.frame.background);
    let click_entity = entity.clone();
    let title = match prompt.label.as_str() {
        ":" => "Command",
        "/" => "Search",
        "?" => "Search backwards",
        ">" | "" => "Filter",
        label => label,
    }
    .to_owned();
    div()
        .id("native-input")
        .flex()
        .items_center()
        .w_full()
        .h(px(48.))
        .px_4()
        .gap_2()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            click_entity.update(cx, |view, _| {
                window.focus(&view.focus);
                view.place_prompt_cursor(event.position);
            });
            cx.stop_propagation();
        })
        .child(
            div()
                .text_color(palette.accent)
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .child(
            canvas(
                move |bounds, window, cx| {
                    let cursor = prompt.cursor.min(prompt.text.len());
                    let mut content = prompt.text.clone();
                    content.insert_str(cursor, &preedit);
                    let caret_index = cursor + preedit.len();
                    let mut runs = Vec::new();
                    if content.is_empty() {
                        content = prompt.suggestion.clone().unwrap_or_default();
                        runs.push(text_run(&content, style.add_modifier(Modifier::DIM), &font));
                    } else {
                        let mut offset = 0;
                        let mut inserted = false;
                        for segment in &prompt.styled_text.0 {
                            let end = offset + segment.text.len();
                            if !inserted && cursor >= offset && cursor <= end {
                                let split = cursor - offset;
                                runs.push(text_run(
                                    &segment.text[..split],
                                    style.patch(segment.style),
                                    &font,
                                ));
                                if !preedit.is_empty() {
                                    let mut run = text_run(&preedit, style, &font);
                                    run.underline = Some(UnderlineStyle {
                                        color: None,
                                        thickness: px(1.),
                                        wavy: false,
                                    });
                                    runs.push(run);
                                }
                                runs.push(text_run(
                                    &segment.text[split..],
                                    style.patch(segment.style),
                                    &font,
                                ));
                                inserted = true;
                            } else {
                                runs.push(text_run(
                                    &segment.text,
                                    style.patch(segment.style),
                                    &font,
                                ));
                            }
                            offset = end;
                        }
                        if runs.iter().map(|run| run.len).sum::<usize>() != content.len() {
                            runs = vec![text_run(&content, style, &font)];
                        }
                    }
                    let line =
                        window
                            .text_system()
                            .shape_line(content.into(), font_size, &runs, None);
                    let caret_x = line.x_for_index(caret_index);
                    let offset = (caret_x - bounds.size.width + px(8.)).max(px(0.));
                    let origin =
                        bounds.origin + point(-offset, (bounds.size.height - line_height) / 2.);
                    let caret =
                        Bounds::new(origin + point(caret_x, px(0.)), size(px(2.), line_height));
                    entity.update(cx, |view, _| {
                        view.native_caret = Some(caret);
                        view.prompt_hit =
                            Some((prompt.id, line.clone(), origin, prompt.text.len()));
                    });
                    (line, origin, caret)
                },
                move |bounds, (line, origin, caret), window, cx| {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        if let Err(error) = line.paint(origin, line_height, window, cx) {
                            log::error!("Prompt paint failed: {error}");
                        }
                        window.paint_quad(fill(caret, palette.accent));
                    });
                },
            )
            .h_full()
            .flex_1()
            .min_w_0(),
        )
}

pub fn render(
    widget: &frontend::Widget,
    index: usize,
    view: &EditorView,
    entity: Entity<EditorView>,
    events: UnboundedSender<ApplicationEvent>,
) -> AnyElement {
    let palette = crate::design::Palette::for_background(view.frame.background);
    let area = widget.area;
    let font = view.ui_font.clone();
    let line_height = view.line_height;
    let row_height = px(28.);
    let style = widget.style;
    let selected_style = style.patch(widget.selected_style);
    let x = view.cell_width * area.x as f32;
    let y = line_height * area.y as f32;
    let width = view.cell_width * area.width as f32;
    let height = line_height * area.height as f32;
    match &widget.content {
        frontend::WidgetContent::Tabs(tabs) => {
            let mut bar = div()
                .id("buffer-tabs")
                .absolute()
                .left(x)
                .top(y)
                .w(width)
                .h(height)
                .flex()
                .items_center()
                .overflow_x_scroll()
                .px_2()
                .gap_1()
                .font_family(font.family.clone())
                .text_size(px(13.))
                .bg(palette.shell);
            for (index, tab) in tabs.iter().enumerate() {
                let id = tab.id;
                let activate = events.clone();
                let close = events.clone();
                bar = bar.child(
                    div()
                        .id(("buffer-tab", index))
                        .flex()
                        .items_center()
                        .flex_none()
                        .h(px(30.))
                        .gap_2()
                        .px_3()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(if tab.active {
                            palette.border
                        } else {
                            palette.shell
                        })
                        .text_color(if tab.active {
                            palette.text
                        } else {
                            palette.muted
                        })
                        .when(tab.active, |tab| tab.bg(palette.raised))
                        .hover(|tab| tab.bg(palette.hover).text_color(palette.text))
                        .cursor_pointer()
                        .child(
                            div()
                                .w(px(9.))
                                .h(px(12.))
                                .rounded(px(2.))
                                .border_1()
                                .border_color(palette.muted),
                        )
                        .child(tab.label.clone())
                        .when(tab.modified, |tab| {
                            tab.child(div().size(px(5.)).rounded_full().bg(palette.accent))
                        })
                        .child(
                            div()
                                .id(("close-tab", index))
                                .px_1()
                                .rounded_sm()
                                .text_color(palette.muted)
                                .hover(|s| s.bg(palette.hover).text_color(palette.text))
                                .child("×")
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    let _ = close.send(ApplicationEvent::Ui(
                                        frontend::UiEvent::CloseTab(id),
                                    ));
                                    cx.stop_propagation();
                                }),
                        )
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            let _ = activate
                                .send(ApplicationEvent::Ui(frontend::UiEvent::ActivateTab(id)));
                            cx.stop_propagation();
                        }),
                );
            }
            let create = events.clone();
            bar = bar.child(
                div()
                    .id("new-tab")
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .text_color(palette.muted)
                    .text_size(px(18.))
                    .cursor_pointer()
                    .hover(|s| s.bg(palette.hover).text_color(palette.text))
                    .child("+")
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        let _ = create.send(ApplicationEvent::Command(":new".parse().unwrap()));
                        cx.stop_propagation();
                    }),
            );
            return bar.into_any_element();
        }
        frontend::WidgetContent::Status {
            left,
            center,
            right,
        } => {
            return div()
                .absolute()
                .left(x)
                .top(if area.bottom() == view.frame.buffer.area.bottom() {
                    (view.bounds.size.height - height).max(px(0.))
                } else {
                    y
                })
                .w(width)
                .h(height)
                .flex()
                .items_center()
                .justify_between()
                .whitespace_nowrap()
                .px_2()
                .overflow_hidden()
                .bg(color(style.bg.unwrap_or(Color::Reset), 0x252936))
                .font_family(font.family.clone())
                .text_size(px(13.))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .child(label(left, style, &font)),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .child(label(center, style, &font)),
                )
                .child(div().flex_none().child(label(right, style, &font)))
                .into_any_element();
        }
        frontend::WidgetContent::Divider(divider) => {
            let vertical = divider.layout == helix_view::tree::Layout::Vertical;
            let divider = divider.clone();
            return div()
                .id(("split-divider", index))
                .absolute()
                .left(if vertical { x - px(2.) } else { x })
                .top(if vertical { y } else { y - px(2.) })
                .w(if vertical { px(5.) } else { width })
                .h(if vertical { height } else { px(5.) })
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(if vertical { px(1.) } else { width })
                        .h(if vertical { height } else { px(1.) })
                        .bg(palette.border),
                )
                .hover(|s| s.bg(palette.border))
                .cursor(if vertical {
                    CursorStyle::ResizeLeftRight
                } else {
                    CursorStyle::ResizeUpDown
                })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    entity.update(cx, |view, _| {
                        view.split_drag = Some(divider.clone());
                        window.focus(&view.focus);
                    });
                    cx.stop_propagation();
                })
                .into_any_element();
        }
        frontend::WidgetContent::Buffer(buffer) => {
            return div()
                .absolute()
                .left(x + px(4.))
                .top(y)
                .w(width)
                .h(height)
                .child(crate::surface::BufferElement {
                    view: entity,
                    preview: Some(buffer.clone()),
                })
                .border_1()
                .rounded(px(10.))
                .border_color(palette.border)
                .into_any_element();
        }
        frontend::WidgetContent::Border => {
            return div()
                .absolute()
                .left(x)
                .top(y)
                .w(width)
                .h(height)
                .border_1()
                .rounded(px(10.))
                .border_color(palette.border)
                .into_any_element();
        }
        _ => (),
    }
    let mut panel = div()
        .id(("native-panel", index))
        .absolute()
        .left(view.cell_width * area.x as f32)
        .w(view.cell_width * area.width as f32)
        .flex()
        .flex_col()
        .overflow_hidden()
        .font_family(font.family.clone())
        .font_family(font.family.clone())
        .text_size(px(13.))
        .line_height(line_height)
        .bg(palette.panel)
        .text_color(palette.text)
        .border_1()
        .border_color(palette.border)
        .rounded(px(10.))
        .shadow_lg()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    match &widget.content {
        frontend::WidgetContent::Document { title, blocks } => {
            let scroll = scroll_handle(view, index, widget);
            // Native headings and code blocks need room for their padding even
            // when the engine's text measurement only contains one line.
            let height = height
                .max(line_height * if title.is_empty() { 3. } else { 5. })
                .min(view.bounds.size.height);
            panel = panel
                .top(y.min((view.bounds.size.height - height).max(px(0.))))
                .h(height);
            if !title.is_empty() {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .flex_none()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .font_weight(FontWeight::BOLD)
                        .child(title.clone()),
                );
            }
            let mut document = div()
                .id(("documentation", index))
                .track_scroll(&scroll)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_2();
            for (block_index, block) in blocks.iter().enumerate() {
                if matches!(block.kind, frontend::BlockKind::Rule) {
                    document = document.child(div().h(px(1.)).my_2().w_full().bg(palette.border));
                    continue;
                }
                let links = block.links.clone();
                let text = InteractiveText::new(
                    ("doc-text", block_index),
                    label(&block.text, style, &font),
                )
                .on_click(
                    links.iter().map(|(range, _)| range.clone()).collect(),
                    move |i, _, cx| {
                        if let Some((_, url)) = links.get(i) {
                            cx.open_url(url);
                        }
                    },
                );
                let mut row = div().mb_2().min_w_0();
                match &block.kind {
                    frontend::BlockKind::Heading => {
                        row = row
                            .font_weight(FontWeight::BOLD)
                            .text_size(view.font_size * 1.15);
                    }
                    frontend::BlockKind::Code => {
                        row = row
                            .p_2()
                            .rounded(px(6.))
                            .bg(palette.raised)
                            .font_family(view.font.family.clone())
                            .overflow_hidden();
                    }
                    frontend::BlockKind::Quote => {
                        row = row.pl_2().border_l_2().border_color(palette.border);
                    }
                    frontend::BlockKind::Item(marker) => {
                        row = row.flex().gap_2().child(marker.clone());
                    }
                    _ => (),
                }
                document = document.child(row.child(div().min_w_0().child(text)));
            }
            panel = panel.child(document);
        }
        frontend::WidgetContent::Text { title, text } => {
            let scroll = scroll_handle(view, index, widget);
            let height = height
                .max(line_height * if title.is_empty() { 2. } else { 4. })
                .min(view.bounds.size.height);
            panel = panel
                .top(y.min((view.bounds.size.height - height).max(px(0.))))
                .h(height);
            if !title.is_empty() {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .flex_none()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .font_weight(FontWeight::BOLD)
                        .child(title.clone()),
                );
            }
            panel = panel.child(
                div()
                    .id(("native-documentation", index))
                    .track_scroll(&scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .child(label(text, style, &font)),
            );
        }
        frontend::WidgetContent::Hints { title, entries } => {
            let height = (px(28.) * entries.len() as f32 + px(42.))
                .min((view.bounds.size.height - px(24.)).max(px(0.)));
            panel = panel
                .top(y.min((view.bounds.size.height - height - px(12.)).max(px(0.))))
                .h(height)
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .font_weight(FontWeight::BOLD)
                        .child(title.clone()),
                );
            let mut rows = div()
                .id(("key-hints", index))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll();
            for (row, (keys, description)) in entries.iter().enumerate() {
                let keys = keys.clone();
                let events = events.clone();
                rows = rows.child(
                    div()
                        .id(("key-hint", row))
                        .flex()
                        .items_center()
                        .px_2()
                        .gap_2()
                        .min_h(px(28.))
                        .cursor_pointer()
                        .hover(|s| s.bg(palette.hover))
                        .child(
                            div()
                                .min_w(px(56.))
                                .px_2()
                                .rounded(px(4.))
                                .bg(palette.raised)
                                .text_color(palette.muted)
                                .flex_none()
                                .font_family(view.font.family.clone())
                                .text_size(px(11.))
                                .child(keys.clone()),
                        )
                        .child(description.clone())
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            if let Some(key) =
                                keys.split(", ").next().and_then(|key| key.parse().ok())
                            {
                                let _ = events
                                    .send(ApplicationEvent::Ui(frontend::UiEvent::Keys(vec![key])));
                            }
                            cx.stop_propagation();
                        }),
                );
            }
            panel = panel.child(rows);
        }
        frontend::WidgetContent::Menu(menu) => {
            let height = (row_height * menu.rows.len().max(1) as f32 + px(2.))
                .min(view.bounds.size.height - px(8.));
            panel = panel
                .top(y.min((view.bounds.size.height - height).max(px(0.))))
                .h(height);
            let rows = menu.rows.clone();
            let offset = menu.offset;
            let selected = menu.selected;
            let id = menu.id;
            let total = menu.total;
            let scroll = UniformListScrollHandle::new();
            scroll.scroll_to_item(
                selected.unwrap_or(offset).saturating_sub(offset),
                ScrollStrategy::Top,
            );
            let scroll_events = events.clone();
            panel = panel.on_scroll_wheel(move |event, _, cx| {
                if total > 0 {
                    let delta = if event.delta.pixel_delta(line_height).y > px(0.) {
                        -1
                    } else {
                        1
                    };
                    let index =
                        (selected.unwrap_or(0) as i64 + delta).clamp(0, total as i64 - 1) as usize;
                    let _ = scroll_events.send(ApplicationEvent::Ui(frontend::UiEvent::Menu {
                        id,
                        index,
                        accept: false,
                    }));
                }
                cx.stop_propagation();
            });
            panel = panel.child(
                uniform_list(
                    ("completion-menu", index),
                    rows.len(),
                    move |range, _, _| {
                        range
                            .map(|row| {
                                let index = offset + row;
                                let base = if selected == Some(index) {
                                    selected_style
                                } else {
                                    style
                                };
                                let events = events.clone();
                                div()
                                    .id(("menu-row", row))
                                    .flex()
                                    .gap_2()
                                    .px_2()
                                    .h(row_height)
                                    .cursor_pointer()
                                    .overflow_hidden()
                                    .rounded(px(5.))
                                    .bg(if selected == Some(index) {
                                        palette.raised
                                    } else {
                                        palette.panel
                                    })
                                    .hover(|s| s.bg(palette.hover))
                                    .children(rows[row].iter().map(|text| label(text, base, &font)))
                                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                        let _ = events.send(ApplicationEvent::Ui(
                                            frontend::UiEvent::Menu {
                                                id,
                                                index,
                                                accept: true,
                                            },
                                        ));
                                        cx.stop_propagation();
                                    })
                            })
                            .collect()
                    },
                )
                .track_scroll(scroll)
                .flex_1()
                .min_h_0(),
            );
        }
        frontend::WidgetContent::Tabs(_)
        | frontend::WidgetContent::Status { .. }
        | frontend::WidgetContent::Divider(_)
        | frontend::WidgetContent::Border
        | frontend::WidgetContent::Buffer(_) => unreachable!(),
        frontend::WidgetContent::Picker(picker) => {
            panel = panel
                .left(x)
                .w((width - px(8.)).max(px(0.)))
                .top(line_height * area.y as f32)
                .h(line_height * area.height as f32);
            let scroll_events = events.clone();
            let picker_id = picker.id;
            panel = panel.on_scroll_wheel(move |event, _, cx| {
                let delta = event.delta.pixel_delta(line_height).y;
                if delta != px(0.) {
                    let lines = -(delta / line_height).abs().ceil().max(1.) as i32
                        * if delta > px(0.) { 1 } else { -1 };
                    let _ =
                        scroll_events.send(ApplicationEvent::Ui(frontend::UiEvent::ScrollPicker {
                            id: picker_id,
                            lines,
                        }));
                }
                cx.stop_propagation();
            });
            let count = format!(
                "{}{}/{}",
                if picker.running { "… " } else { "" },
                picker.matched,
                picker.total
            );
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(palette.border)
                    .child(div().flex_1().min_w_0().child(prompt_input(
                        picker.prompt.clone(),
                        style,
                        view,
                        entity,
                    )))
                    .child(
                        div()
                            .px_3()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(palette.muted)
                            .child(count),
                    ),
            );
            let widths: Vec<_> = picker
                .widths
                .iter()
                .map(|width| view.cell_width * (*width as f32 + 2.))
                .collect();
            if !picker.headers.is_empty() {
                let mut header = div()
                    .flex()
                    .items_center()
                    .h(row_height)
                    .px_3()
                    .text_size(px(11.))
                    .text_color(palette.muted)
                    .overflow_hidden();
                for (i, title) in picker.headers.iter().enumerate() {
                    header = header.child(
                        div()
                            .w(widths[i])
                            .flex_none()
                            .overflow_hidden()
                            .child(label(title, style, &font)),
                    );
                }
                panel = panel.child(header);
            }
            let rows = picker.rows.clone();
            let selected = picker.selected;
            let id = picker.id;
            let scroll = UniformListScrollHandle::new();
            scroll.scroll_to_item(
                rows.iter()
                    .position(|row| row.index == selected)
                    .unwrap_or(0),
                ScrollStrategy::Top,
            );
            if rows.is_empty() {
                panel = panel.child(div().p_3().child(if picker.running {
                    "Searching…"
                } else {
                    "No matches"
                }));
            } else {
                panel = panel.child(
                    uniform_list(("native-picker", index), rows.len(), move |range, _, _| {
                        range
                            .map(|row| {
                                let item = &rows[row];
                                let selected = item.index == selected;
                                let base = if selected { selected_style } else { style };
                                let mut element = div()
                                    .id(("native-row", row))
                                    .flex()
                                    .w_full()
                                    .h(row_height)
                                    .items_center()
                                    .px_3()
                                    .rounded(px(5.))
                                    .overflow_hidden()
                                    .bg(if selected {
                                        palette.raised
                                    } else {
                                        palette.panel
                                    })
                                    .cursor_pointer()
                                    .hover(|s| s.bg(palette.hover));
                                for (i, column) in item.columns.iter().enumerate() {
                                    element = element.child(
                                        div()
                                            .w(widths[i])
                                            .flex_none()
                                            .overflow_hidden()
                                            .child(label(column, base, &font)),
                                    );
                                }
                                let events = events.clone();
                                let item_index = item.index;
                                element.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    let _ = events.send(ApplicationEvent::Ui(
                                        frontend::UiEvent::ActivatePicker {
                                            id,
                                            index: item_index,
                                        },
                                    ));
                                    cx.stop_propagation();
                                })
                            })
                            .collect()
                    })
                    .track_scroll(scroll)
                    .flex_1()
                    .min_h_0(),
                );
            }
        }
        frontend::WidgetContent::Prompt(prompt) => {
            let width = (view.bounds.size.width - px(48.)).min(px(680.)).max(px(0.));
            let top =
                (line_height * 2. + px(16.)).min((view.bounds.size.height - px(48.)).max(px(0.)));
            panel = panel
                .left((view.bounds.size.width - width) / 2.)
                .w(width)
                .top(top)
                .max_h((view.bounds.size.height - top - px(16.)).max(px(48.)))
                .child(
                    div()
                        .border_b_1()
                        .border_color(palette.border)
                        .child(prompt_input(prompt.clone(), style, view, entity)),
                );
            if let Some(documentation) = &prompt.documentation {
                panel = panel.child(
                    div()
                        .id(("prompt-doc", index))
                        .p_2()
                        .max_h(line_height * 6.)
                        .overflow_y_scroll()
                        .child(documentation.clone()),
                );
            }
            if !prompt.completions.is_empty() {
                let items = prompt.completions.clone();
                let selected = prompt.selected;
                let visible = items.len().min(8);
                let offset = selected.unwrap_or(0) / visible * visible;
                let end = (offset + visible).min(items.len());
                let id = prompt.id;
                panel = panel.child(
                    uniform_list(
                        ("prompt-completions", index),
                        end - offset,
                        move |range, _, _| {
                            range
                                .map(|row| {
                                    let index = offset + row;
                                    let selected = selected == Some(index);
                                    let base = if selected { selected_style } else { style };
                                    let events = events.clone();
                                    div()
                                        .id(("completion", row))
                                        .h(row_height)
                                        .px_2()
                                        .overflow_hidden()
                                        .cursor_pointer()
                                        .rounded(px(5.))
                                        .bg(if selected {
                                            palette.raised
                                        } else {
                                            palette.panel
                                        })
                                        .hover(|s| s.bg(palette.hover))
                                        .child(label(&items[index], base, &font))
                                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                            let _ = events.send(ApplicationEvent::Ui(
                                                frontend::UiEvent::CompletePrompt { id, index },
                                            ));
                                            cx.stop_propagation();
                                        })
                                })
                                .collect()
                        },
                    )
                    .h(row_height * (end - offset) as f32)
                    .flex_none(),
                );
            }
            panel = panel.child(
                div()
                    .h(px(30.))
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_t_1()
                    .border_color(palette.border)
                    .text_color(palette.muted)
                    .text_size(px(11.))
                    .child("↑ ↓  Navigate")
                    .child("↵  Apply")
                    .child("esc  Close"),
            );
        }
    }
    panel.into_any_element()
}
