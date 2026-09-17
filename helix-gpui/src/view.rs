use crate::{
    backend::Frame,
    input,
    surface::{color, BufferElement},
};
use gpui::{prelude::*, *};
use helix_term::application::ApplicationEvent;
use helix_view::{
    graphics::Rect,
    input::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton as HelixButton,
        MouseEvent as HelixMouse, MouseEventKind,
    },
};
use std::{ops::Range, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender, watch};

#[derive(Clone, Debug, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub struct RunCommand {
    pub command: String,
}

const MENUS: &[(&str, &[(&str, &str)])] = &[
    (
        "File",
        &[
            ("New", ":new"),
            ("Open…", "@open"),
            ("File picker", "file_picker"),
            ("Save", ":write"),
            ("Save as…", "@save-as"),
            ("Save all", ":write-all"),
            ("Close buffer", ":buffer-close"),
            ("Quit", ":quit-all"),
        ],
    ),
    (
        "Edit",
        &[
            ("Undo", "undo"),
            ("Redo", "redo"),
            ("Copy", "yank_to_clipboard"),
            ("Paste", "@paste"),
            ("Select all", "select_all"),
            ("Format", ":format"),
        ],
    ),
    (
        "Selection",
        &[
            ("Select line", "extend_to_line_bounds"),
            ("Split into lines", "split_selection_on_newline"),
            ("Keep primary", "keep_primary_selection"),
            ("Invert", "flip_selections"),
            ("Select matches", "select_regex"),
        ],
    ),
    (
        "Search",
        &[
            ("Search", "search"),
            ("Search backwards", "rsearch"),
            ("Next", "search_next"),
            ("Previous", "search_prev"),
            ("Global search", "global_search"),
            ("Command palette", "command_palette"),
        ],
    ),
    (
        "View",
        &[
            ("Buffers", "buffer_picker"),
            ("Vertical split", ":vsplit"),
            ("Horizontal split", ":hsplit"),
            ("Next split", "rotate_view"),
            ("Close split", ":quit"),
            ("Zoom in", "@zoom-in"),
            ("Zoom out", "@zoom-out"),
            ("Reset zoom", "@zoom-reset"),
        ],
    ),
    (
        "Code",
        &[
            ("Definition", "goto_definition"),
            ("References", "goto_reference"),
            ("Hover", "hover"),
            ("Rename", "rename_symbol"),
            ("Code actions", "code_action"),
            ("Document symbols", "symbol_picker"),
            ("Workspace symbols", "workspace_symbol_picker"),
            ("Diagnostics", "diagnostics_picker"),
            ("Workspace diagnostics", "workspace_diagnostics_picker"),
        ],
    ),
    (
        "Debug",
        &[
            ("Launch", "dap_launch"),
            ("Continue", "dap_continue"),
            ("Pause", "dap_pause"),
            ("Step over", "dap_next"),
            ("Step in", "dap_step_in"),
            ("Step out", "dap_step_out"),
            ("Toggle breakpoint", "dap_toggle_breakpoint"),
            ("Variables", "dap_variables"),
            ("Threads", "dap_switch_thread"),
            ("Stack frames", "dap_switch_stack_frame"),
            ("Stop", "dap_terminate"),
        ],
    ),
];

pub fn install_menus(cx: &mut App) {
    cx.set_menus(
        MENUS
            .iter()
            .map(|(name, items)| Menu {
                name: (*name).into(),
                items: items
                    .iter()
                    .map(|(label, command)| {
                        MenuItem::action(
                            *label,
                            RunCommand {
                                command: (*command).into(),
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
    );
    let prefix = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl-shift"
    };
    cx.bind_keys(
        [
            ("o", "@open"),
            ("s", ":write"),
            ("p", "command_palette"),
            ("q", ":quit-all"),
        ]
        .map(|(key, command)| {
            KeyBinding::new(
                &format!("{prefix}-{key}"),
                RunCommand {
                    command: command.into(),
                },
                Some("Helix"),
            )
        }),
    );
}

pub struct EditorView {
    events: UnboundedSender<ApplicationEvent>,
    dimensions: Rect,
    pub frame: Arc<Frame>,
    pub focus: FocusHandle,
    pub bounds: Bounds<Pixels>,
    pub font_size: Pixels,
    pub font: Font,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub preedit: String,
    preedit_selection: Range<usize>,
    menu: Option<usize>,
    menu_cursor: usize,
    scroll: Point<Pixels>,
    error: Option<String>,
}

impl EditorView {
    pub fn new(
        events: UnboundedSender<ApplicationEvent>,
        mut frames: watch::Receiver<Arc<Frame>>,
        dimensions: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let fonts = cx.text_system().all_font_names();
        let family = std::env::var("HELIX_GPUI_FONT")
            .ok()
            .or_else(|| {
                [
                    "JetBrains Mono",
                    "Ioskeley Mono",
                    "Iosevka",
                    "DejaVu Sans Mono",
                    "Menlo",
                    "Consolas",
                    "Liberation Mono",
                ]
                .into_iter()
                .find(|name| fonts.iter().any(|font| font == name))
                .map(str::to_owned)
            })
            .or_else(|| {
                fonts
                    .into_iter()
                    .find(|name| name.to_lowercase().contains("mono"))
            })
            .unwrap_or_else(|| "monospace".into());
        let font_size = std::env::var("HELIX_GPUI_FONT_SIZE")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite())
            .unwrap_or(15.)
            .clamp(8., 40.);
        window.focus(&focus);
        let frame = frames.borrow_and_update().clone();
        cx.spawn(async move |this, cx| {
            while frames.changed().await.is_ok() {
                let frame = frames.borrow_and_update().clone();
                if this
                    .update(cx, |this, cx| {
                        if let Some(result) = &frame.exit {
                            match result {
                                Ok(_) => cx.quit(),
                                Err(error) => {
                                    this.error = Some(error.clone());
                                    log::error!("{error}");
                                }
                            }
                        }
                        this.frame = frame;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let close_events = events.clone();
        window.on_window_should_close(cx, move |_, _| {
            let Ok(command) = ":quit-all".parse() else {
                return false;
            };
            // Helix itself checks modified buffers; failed quit leaves the window open.
            close_events
                .send(ApplicationEvent::Command(command))
                .is_err()
        });
        cx.observe_window_activation(window, |this, window, _| {
            this.send(if window.is_window_active() {
                Event::FocusGained
            } else {
                Event::FocusLost
            });
        })
        .detach();
        let send_theme = |events: &UnboundedSender<ApplicationEvent>, appearance| {
            let mode = match appearance {
                WindowAppearance::Light | WindowAppearance::VibrantLight => {
                    helix_view::theme::Mode::Light
                }
                _ => helix_view::theme::Mode::Dark,
            };
            let _ = events.send(ApplicationEvent::Theme(mode));
        };
        send_theme(&events, window.appearance());
        cx.observe_window_appearance(window, move |this, window, _| {
            send_theme(&this.events, window.appearance());
        })
        .detach();
        Self {
            events,
            dimensions,
            frame,
            focus,
            bounds: Bounds::default(),
            font_size: px(font_size),
            font: font(family),
            cell_width: px(9.),
            line_height: px(21.),
            preedit: String::new(),
            preedit_selection: 0..0,
            menu: None,
            menu_cursor: 0,
            scroll: point(px(0.), px(0.)),
            error: None,
        }
    }
    pub fn send(&self, event: Event) {
        let _ = self.events.send(ApplicationEvent::Input(event));
    }
    fn type_text(&self, text: &str) {
        for ch in text.chars() {
            self.send(Event::Key(KeyEvent {
                code: match ch {
                    '\n' | '\r' => KeyCode::Enter,
                    '\t' => KeyCode::Tab,
                    _ => KeyCode::Char(ch),
                },
                modifiers: KeyModifiers::empty(),
            }));
        }
    }
    fn command(&self, command: &str) {
        match command.parse() {
            Ok(command) => {
                let _ = self.events.send(ApplicationEvent::Command(command));
            }
            Err(error) => log::error!("Invalid menu command {command}: {error}"),
        }
    }
    fn run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        match action.command.as_str() {
            "@open" => {
                let paths = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: true,
                    multiple: true,
                    prompt: Some("Open in Helix".into()),
                });
                let events = self.events.clone();
                cx.spawn(async move |_, _| {
                    if let Ok(Ok(Some(paths))) = paths.await {
                        for path in paths {
                            let _ = events.send(ApplicationEvent::Open(path));
                        }
                    }
                })
                .detach();
            }
            "@save-as" => {
                let path =
                    cx.prompt_for_new_path(&std::env::current_dir().unwrap_or_default(), None);
                let events = self.events.clone();
                cx.spawn(async move |_, _| {
                    if let Ok(Ok(Some(path))) = path.await {
                        // Typable commands use single-quote doubling for literal paths.
                        let command =
                            format!(":write '{}'", path.to_string_lossy().replace('\'', "''"));
                        if let Ok(command) = command.parse() {
                            let _ = events.send(ApplicationEvent::Command(command));
                        }
                    }
                })
                .detach();
            }
            "@paste" => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.send(Event::Paste(text));
                }
            }
            "@zoom-in" => self.font_size = (self.font_size + px(1.)).min(px(40.)),
            "@zoom-out" => self.font_size = (self.font_size - px(1.)).max(px(8.)),
            "@zoom-reset" => self.font_size = px(15.),
            command => self.command(command),
        }
        window.focus(&self.focus);
        cx.notify();
    }
    pub fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, _cx: &mut Context<Self>) {
        let run = TextRun {
            len: 1,
            font: self.font.clone(),
            color: rgb(0xffffff).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        self.cell_width = window
            .text_system()
            .shape_line("M".into(), self.font_size, &[run], None)
            .width
            .max(px(1.));
        self.line_height = (self.font_size * 1.4).ceil();
        self.bounds = bounds;
        let cols = (bounds.size.width / self.cell_width)
            .floor()
            .clamp(1., 1000.) as u16;
        let rows = (bounds.size.height / self.line_height)
            .floor()
            .clamp(1., 1000.) as u16;
        let area = Rect::new(0, 0, cols, rows);
        if self.dimensions != area {
            self.dimensions = area;
            self.send(Event::Resize(cols, rows));
        }
    }
    fn mouse(&self, position: Point<Pixels>, mods: Modifiers, kind: MouseEventKind) {
        let column = ((position.x - self.bounds.left()) / self.cell_width)
            .floor()
            .max(0.) as u16;
        let row = ((position.y - self.bounds.top()) / self.line_height)
            .floor()
            .max(0.) as u16;
        self.send(Event::Mouse(HelixMouse {
            kind,
            column: column.min(self.frame.buffer.area.width.saturating_sub(1)),
            row: row.min(self.frame.buffer.area.height.saturating_sub(1)),
            modifiers: input::modifiers(mods),
        }));
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "f10" {
            self.menu = self.menu.is_none().then_some(0);
            self.menu_cursor = 0;
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
            return;
        }
        if let Some(menu) = self.menu {
            let count = MENUS[menu].1.len();
            match event.keystroke.key.as_str() {
                "escape" => self.menu = None,
                "up" => self.menu_cursor = (self.menu_cursor + count - 1) % count,
                "down" => self.menu_cursor = (self.menu_cursor + 1) % count,
                "left" => {
                    self.menu = Some((menu + MENUS.len() - 1) % MENUS.len());
                    self.menu_cursor = 0;
                }
                "right" => {
                    self.menu = Some((menu + 1) % MENUS.len());
                    self.menu_cursor = 0;
                }
                "enter" | "space" => self.run_command(
                    &RunCommand {
                        command: MENUS[menu].1[self.menu_cursor].1.into(),
                    },
                    window,
                    cx,
                ),
                _ => (),
            }
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
            return;
        }
        if event.keystroke.key == "escape" && !self.preedit.is_empty() {
            self.preedit.clear();
            cx.notify();
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Let the platform deliver printable text (including IME) through the
        // input handler. Commands and special keys go directly to Helix.
        if event.keystroke.key_char.is_some()
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.alt
        {
            return;
        }
        if let Some(key) = input::key(&event.keystroke) {
            self.send(Event::Key(key));
            cx.stop_propagation();
            window.prevent_default();
        }
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("helix")
            .key_context("Helix")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(color(self.frame.background, 0x1b1d27))
            .text_color(rgb(0xd8dee9))
            .text_size(px(13.))
            .on_action(cx.listener(Self::run_command))
            .on_key_down(cx.listener(Self::key_down));
        let mut bar = div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .bg(rgb(0x252936));
        for (i, (name, _)) in MENUS.iter().enumerate() {
            bar = bar.child(
                div()
                    .id(("menu", i))
                    .px_3()
                    .h_full()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x3a4054)))
                    .child(*name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.menu_cursor = 0;
                        this.menu = if this.menu == Some(i) { None } else { Some(i) };
                        cx.notify();
                    })),
            );
        }
        root = root.child(bar);
        let mut content = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.menu = None;
                    window.focus(&this.focus);
                    this.mouse(
                        event.position,
                        event.modifiers,
                        MouseEventKind::Down(HelixButton::Left),
                    );
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, _| {
                    this.mouse(
                        event.position,
                        event.modifiers,
                        MouseEventKind::Up(HelixButton::Left),
                    )
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, _| {
                if event.pressed_button == Some(MouseButton::Left) {
                    this.mouse(
                        event.position,
                        event.modifiers,
                        MouseEventKind::Drag(HelixButton::Left),
                    );
                }
            }))
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, event: &MouseUpEvent, _, _| {
                    this.mouse(
                        event.position,
                        event.modifiers,
                        MouseEventKind::Up(HelixButton::Right),
                    );
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseUpEvent, _, _| {
                    this.mouse(
                        event.position,
                        event.modifiers,
                        MouseEventKind::Up(HelixButton::Middle),
                    );
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, _| {
                this.scroll += event.delta.pixel_delta(this.line_height);
                while this.scroll.y.abs() >= this.line_height {
                    let up = this.scroll.y > px(0.);
                    this.mouse(
                        event.position,
                        event.modifiers,
                        if up {
                            MouseEventKind::ScrollUp
                        } else {
                            MouseEventKind::ScrollDown
                        },
                    );
                    this.scroll.y += if up {
                        -this.line_height
                    } else {
                        this.line_height
                    };
                }
                while this.scroll.x.abs() >= this.cell_width {
                    let left = this.scroll.x > px(0.);
                    this.mouse(
                        event.position,
                        event.modifiers,
                        if left {
                            MouseEventKind::ScrollLeft
                        } else {
                            MouseEventKind::ScrollRight
                        },
                    );
                    this.scroll.x += if left {
                        -this.cell_width
                    } else {
                        this.cell_width
                    };
                }
            }))
            .on_drop::<ExternalPaths>(cx.listener(|this, paths: &ExternalPaths, _, _| {
                for path in paths.paths() {
                    let _ = this.events.send(ApplicationEvent::Open(path.clone()));
                }
            }))
            .child(BufferElement { view: cx.entity() });
        for (index, area) in self.frame.buffer.lists.iter().copied().enumerate() {
            if area.width == 0 || area.height == 0 {
                continue;
            }
            let frame = self.frame.clone();
            let events = self.events.clone();
            let cell_width = self.cell_width;
            let line_height = self.line_height;
            let font_size = self.font_size;
            let family = self.font.family.clone();
            content = content.child(
                uniform_list(
                    ("helix-list", index),
                    area.height as usize,
                    move |range, _, _| {
                        range
                            .map(|row| {
                                let y = area.y + row as u16;
                                let mut item = div()
                                    .id(("list-row", row))
                                    .relative()
                                    .h(line_height)
                                    .w_full()
                                    .font_family(family.clone())
                                    .text_size(font_size)
                                    .line_height(line_height);
                                let mut x = area.x;
                                while x < area.right().min(frame.buffer.area.right()) {
                                    let cell = &frame.buffer[(x, y)];
                                    let start = x;
                                    let style = cell.style();
                                    let mut text = String::new();
                                    while x < area.right().min(frame.buffer.area.right())
                                        && frame.buffer[(x, y)].style() == style
                                    {
                                        let cell = &frame.buffer[(x, y)];
                                        text.push_str(&cell.symbol);
                                        x += cell.width().max(1) as u16;
                                    }
                                    let mut label = div()
                                        .absolute()
                                        .left(cell_width * (start - area.x) as f32)
                                        .w(cell_width * (x - start) as f32)
                                        .h(line_height)
                                        .bg(color(cell.bg, 0x1b1d27))
                                        .text_color(color(cell.fg, 0xd8dee9))
                                        .child(text);
                                    if cell.modifier.contains(helix_view::graphics::Modifier::BOLD)
                                    {
                                        label = label.font_weight(FontWeight::BOLD);
                                    }
                                    item = item.child(label);
                                }
                                let events = events.clone();
                                item.on_mouse_down(MouseButton::Left, move |event, _, cx| {
                                    let _ = events.send(ApplicationEvent::Input(Event::Mouse(
                                        HelixMouse {
                                            kind: MouseEventKind::Down(HelixButton::Left),
                                            column: area.x,
                                            row: y,
                                            modifiers: input::modifiers(event.modifiers),
                                        },
                                    )));
                                    cx.stop_propagation();
                                })
                            })
                            .collect()
                    },
                )
                .absolute()
                .top(self.line_height * area.y as f32)
                .left(self.cell_width * area.x as f32)
                .w(self.cell_width * area.width as f32)
                .h(self.line_height * area.height as f32),
            );
        }
        root = root.child(content);
        if let Some(i) = self.menu {
            let mut menu = div()
                .id("dropdown")
                .absolute()
                .top(px(28.))
                .left(px(i as f32 * 64.))
                .w(px(240.))
                .py_1()
                .bg(rgb(0x252936))
                .border_1()
                .border_color(rgb(0x555e78))
                .shadow_lg()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.menu = None;
                    cx.notify();
                }));
            for (j, (label, command)) in MENUS[i].1.iter().enumerate() {
                let command = command.to_string();
                menu = menu.child(
                    div()
                        .id(("item", j))
                        .px_3()
                        .py_1()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0x414b68)))
                        .when(j == self.menu_cursor, |s| s.bg(rgb(0x414b68)))
                        .child(*label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_command(
                                &RunCommand {
                                    command: command.clone(),
                                },
                                window,
                                cx,
                            )
                        })),
                );
            }
            root = root.child(menu);
        }
        if let Some(error) = &self.error {
            root = root.child(div().p_4().bg(rgb(0x702020)).child(error.clone()));
        }
        root
    }
}

// Preedit lives in the frontend until committed, so composing text never
// creates partial Helix transactions or pollutes the undo history.
impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = input::utf16_range(&self.preedit, range);
        *actual = Some(
            self.preedit[..range.start].encode_utf16().count()
                ..self.preedit[..range.end].encode_utf16().count(),
        );
        Some(self.preedit[range].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.preedit_selection.clone(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.preedit.is_empty()).then(|| 0..self.preedit.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let text = std::mem::take(&mut self.preedit);
        self.type_text(&text);
        self.preedit_selection = 0..0;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut committed = std::mem::take(&mut self.preedit);
        if let Some(range) = range {
            let range = input::utf16_range(&committed, range);
            committed.replace_range(range, text);
        } else {
            committed = text.into();
        }
        self.preedit_selection = 0..0;
        self.type_text(&committed);
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|range| input::utf16_range(&self.preedit, range))
            .unwrap_or(0..self.preedit.len());
        let start = self.preedit[..range.start].encode_utf16().count();
        self.preedit.replace_range(range, text);
        let end = self.preedit.encode_utf16().count();
        self.preedit_selection = selected
            .map(|range| (start + range.start).min(end)..(start + range.end).min(end))
            .unwrap_or(end..end);
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(Bounds::new(
            self.bounds.origin
                + point(
                    self.cell_width * self.frame.cursor.0 as f32,
                    self.line_height * self.frame.cursor.1 as f32,
                ),
            size(self.cell_width, self.line_height),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::MENUS;
    #[test]
    fn menus_reference_real_helix_commands() {
        for (_, items) in MENUS {
            for (_, command) in *items {
                if !command.starts_with('@') {
                    command
                        .parse::<helix_term::commands::MappableCommand>()
                        .unwrap_or_else(|err| panic!("{command}: {err}"));
                }
            }
        }
    }
}
