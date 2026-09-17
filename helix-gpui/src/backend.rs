//! An in-process surface transport. No PTY, escape sequences, or terminal ownership.
use helix_view::{
    graphics::{CursorKind, Rect},
    theme::{Color, Mode},
};
use std::{io, sync::Arc};
use tokio::sync::watch;
use tui::{
    backend::Backend,
    buffer::{Buffer, Cell},
    terminal::Config,
};

#[derive(Clone, Debug)]
pub struct Frame {
    pub buffer: Buffer,
    pub widgets: Vec<helix_term::frontend::Widget>,
    pub cursor: (u16, u16),
    pub cursor_kind: CursorKind,
    pub background: Color,
    pub exit: Option<Result<i32, String>>,
    pub input_at: Option<std::time::Instant>,
}

impl Frame {
    pub fn from_native(frame: helix_term::frontend::Frame<'_>) -> Self {
        Self {
            buffer: frame.buffer.clone(),
            cursor: frame
                .cursor
                .map(|pos| (pos.col as u16, pos.row as u16))
                .unwrap_or_default(),
            cursor_kind: if frame.widgets.iter().any(|widget| widget.has_input()) {
                CursorKind::Hidden
            } else {
                frame.cursor_kind
            },
            widgets: frame.widgets,
            background: frame.background.bg.unwrap_or(Color::Reset),
            exit: None,
            input_at: frame.input_at,
        }
    }

    pub fn new(area: Rect) -> Self {
        Self {
            buffer: Buffer::empty(area),
            widgets: Vec::new(),
            cursor: (0, 0),
            cursor_kind: CursorKind::Hidden,
            background: Color::Reset,
            exit: None,
            input_at: None,
        }
    }
}

pub struct SurfaceBackend {
    pub size: Rect,
    pub frames: watch::Sender<Arc<Frame>>,
    frame: Frame,
}

impl SurfaceBackend {
    pub fn new(size: Rect, frames: watch::Sender<Arc<Frame>>) -> Self {
        let frame = Frame::new(size);
        Self {
            size,
            frames,
            frame,
        }
    }
}

impl Backend for SurfaceBackend {
    fn set_size(&mut self, area: Rect) {
        self.size = area;
    }
    fn prepare_frame(&mut self, surface: &Buffer) {
        self.frame.buffer.lists.clone_from(&surface.lists);
    }
    fn claim(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn reconfigure(&mut self, _: Config) -> io::Result<()> {
        Ok(())
    }
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        for (x, y, cell) in content {
            self.frame.buffer[(x, y)] = cell.clone();
        }
        Ok(())
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.frame.cursor_kind = CursorKind::Hidden;
        Ok(())
    }
    fn show_cursor(&mut self, kind: CursorKind) -> io::Result<()> {
        self.frame.cursor_kind = kind;
        Ok(())
    }
    fn set_cursor(&mut self, x: u16, y: u16) -> io::Result<()> {
        self.frame.cursor = (x, y);
        Ok(())
    }
    fn clear(&mut self) -> io::Result<()> {
        self.frame.buffer.resize(self.size);
        self.frame.buffer.reset();
        Ok(())
    }
    fn start_sync(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn end_sync(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn size(&self) -> io::Result<Rect> {
        Ok(self.size)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.frames.send_replace(Arc::new(self.frame.clone()));
        Ok(())
    }
    fn supports_true_color(&self) -> bool {
        true
    }
    fn get_theme_mode(&self) -> Option<Mode> {
        None
    }
    fn set_background_color(&mut self, color: Option<Color>) -> io::Result<()> {
        self.frame.background = color.unwrap_or(Color::Reset);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helix_term::{
        application::{Application, ApplicationEvent},
        args::Args,
        config::Config,
    };
    use helix_view::input::{parse_macro, Event};

    #[test]
    fn frames_preserve_wide_cells_styles_cursor_and_list_regions() {
        let area = Rect::new(0, 0, 8, 3);
        let (tx, rx) = watch::channel(Arc::new(Frame::new(area)));
        let backend = SurfaceBackend::new(area, tx);
        let mut terminal = tui::terminal::Terminal::new(backend).unwrap();
        let surface = terminal.current_buffer_mut();
        surface[(0, 0)].set_symbol("界").set_fg(Color::Rgb(1, 2, 3));
        surface.lists.push(Rect::new(0, 1, 8, 1));
        terminal.draw(Some((2, 0)), CursorKind::Bar).unwrap();
        assert_eq!(rx.borrow().buffer[(0, 0)].symbol.as_str(), "界");
        assert_eq!(rx.borrow().buffer[(0, 0)].fg, Color::Rgb(1, 2, 3));
        assert_eq!(rx.borrow().cursor, (2, 0));
        assert_eq!(rx.borrow().cursor_kind, CursorKind::Bar);
        assert_eq!(rx.borrow().buffer.lists.len(), 1);
        let small = Rect::new(0, 0, 1, 1);
        terminal.backend_mut().set_size(small);
        terminal.resize(small).unwrap();
        terminal.draw(None, CursorKind::Hidden).unwrap();
        assert_eq!(rx.borrow().buffer.area, small);
        assert!(rx.borrow().buffer.lists.is_empty());
    }

    fn app(
        path: Option<&std::path::Path>,
    ) -> (Application<SurfaceBackend>, watch::Receiver<Arc<Frame>>) {
        let area = Rect::new(0, 0, 80, 24);
        let (tx, rx) = watch::channel(Arc::new(Frame::new(area)));
        let backend = SurfaceBackend::new(area, tx.clone());
        let config = Config::default();
        let trust = helix_loader::workspace_trust::WorkspaceTrust::new(
            (&config.editor.workspace_trust).into(),
        );
        let mut args = Args::default();
        if let Some(path) = path {
            args.files
                .insert(path.into(), vec![helix_core::Position::new(0, 0)]);
        }
        let mut app = Application::new_with_backend(
            args,
            config,
            helix_core::config::default_lang_loader(),
            trust,
            backend,
            false,
        )
        .unwrap();
        app.set_frontend_renderer(Box::new(move |frame| {
            tx.send_replace(Arc::new(Frame::from_native(frame)));
        }));
        (app, rx)
    }

    fn keys(text: &str) -> Vec<ApplicationEvent> {
        parse_macro(text)
            .unwrap()
            .into_iter()
            .map(|key| ApplicationEvent::Input(Event::Key(key)))
            .collect()
    }
    fn command(text: &str) -> ApplicationEvent {
        ApplicationEvent::Command(text.parse().unwrap())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn graphical_theme_switch_uses_backend_color_capabilities() {
        let (mut app, _) = app(None);
        app.run_frontend(&mut tokio_stream::iter([command(":theme github_light")]))
            .await
            .unwrap();
        assert_eq!(app.editor.theme.name(), "github_light");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_popup_menu_accepts_the_clicked_item_without_cell_output() {
        use helix_term::{
            compositor::{Compositor, Context},
            frontend::{UiEvent, WidgetContent},
            ui::{
                menu::{Item, Row},
                Menu, Popup, PromptEvent as MenuEvent,
            },
        };
        struct Choice(&'static str);
        impl Item for Choice {
            type Data = ();
            fn format(&self, _: &()) -> Row<'_> {
                Row::new([self.0])
            }
        }
        let (mut app, _) = app(None);
        let accepted = Arc::new(std::sync::Mutex::new(String::new()));
        let result = accepted.clone();
        let menu = Menu::new(
            vec![Choice("first"), Choice("界 second")],
            (),
            move |_, choice, event| {
                if event == MenuEvent::Validate {
                    *result.lock().unwrap() = choice.unwrap().0.into();
                }
            },
        );
        let area = Rect::new(0, 0, 80, 24);
        let mut compositor = Compositor::new(area);
        compositor.push(Box::new(
            Popup::new("test-menu", menu).position(Some(helix_core::Position::new(2, 2))),
        ));
        let mut jobs = helix_term::job::Jobs::new();
        let mut context = Context {
            editor: &mut app.editor,
            jobs: &mut jobs,
            scroll: None,
        };
        let mut surface = Buffer::empty(area);
        let widgets = compositor.render_native(area, &mut surface, &mut context);
        assert!(surface
            .content
            .iter()
            .all(|cell| cell.symbol.as_str() == " "));
        let WidgetContent::Menu(menu) = &widgets[0].content else {
            panic!();
        };
        compositor.handle_ui_event(
            &UiEvent::Menu {
                id: menu.id,
                index: 1,
                accept: true,
            },
            &mut context,
        );
        assert_eq!(*accepted.lock().unwrap(), "界 second");
        assert_eq!(compositor.layer_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_markdown_has_rule_elements_and_unicode_link_ranges() {
        use helix_term::{frontend::BlockKind, ui::Markdown};
        let (app, _) = app(None);
        let markdown = Markdown::new(
            "# Heading\n\n[界](https://example.com)\n\n---\n\n```text\ncode\n```".into(),
            app.editor.syn_loader.clone(),
        );
        let blocks = markdown.native_blocks(&app.editor.theme);
        assert!(blocks
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Heading)));
        assert!(blocks
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Rule) && block.text.0.is_empty()));
        assert!(blocks
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Code)));
        let link = blocks.iter().flat_map(|block| &block.links).next().unwrap();
        assert_eq!(link.0, 0.."界".len());
        assert_eq!(link.1, "https://example.com");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_input_does_not_wait_for_background_frame_locks() {
        let (mut app, _) = app(None);
        let runtime = tokio::runtime::Handle::current();
        let (ready, wait) = tokio::sync::oneshot::channel();
        let worker = std::thread::spawn(move || {
            let _runtime = runtime.enter();
            let _guard = helix_event::lock_frame();
            ready.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(500));
        });
        wait.await.unwrap();
        let start = std::time::Instant::now();
        app.run_frontend(&mut tokio_stream::iter(keys("ihello<esc>")))
            .await
            .unwrap();
        let elapsed = start.elapsed();
        worker.join().unwrap();
        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "input waited {elapsed:?} for decorations"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_input_bursts_coalesce_frames_without_dropping_keys() {
        let (mut app, _) = app(None);
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let frames = count.clone();
        app.set_frontend_renderer(Box::new(move |_| {
            frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));
        let input = format!("i{}<esc>", "abc".repeat(100));
        app.run_frontend(&mut tokio_stream::iter(keys(&input)))
            .await
            .unwrap();
        assert_eq!(
            app.editor
                .documents()
                .next()
                .unwrap()
                .text()
                .to_string()
                .trim_end(),
            "abc".repeat(100)
        );
        assert!(
            count.load(std::sync::atomic::Ordering::Relaxed) < 40,
            "rendered every queued key"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_tabs_switch_and_protect_unsaved_documents() {
        use helix_term::frontend::{UiEvent, WidgetContent};
        let (mut app, rx) = app(None);
        app.run_frontend(&mut tokio_stream::iter(keys("ione<esc>:new<ret>itwo<esc>")))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let tabs = frame
            .widgets
            .iter()
            .find_map(|widget| {
                if let WidgetContent::Tabs(tabs) = &widget.content {
                    Some(tabs)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(tabs.len(), 2);
        let first = tabs[0].id;
        app.run_frontend(&mut tokio_stream::iter([
            ApplicationEvent::Ui(UiEvent::ActivateTab(first)),
            ApplicationEvent::Ui(UiEvent::CloseTab(first)),
        ]))
        .await
        .unwrap();
        assert_eq!(app.editor.tree.get(app.editor.tree.focus).doc, first);
        assert_eq!(app.editor.documents().count(), 2);
        assert!(app
            .editor
            .status_msg
            .as_ref()
            .unwrap()
            .0
            .contains("unsaved"));
        // Tab/status labels never appear in the buffer cell surface.
        let frame = rx.borrow().clone();
        for widget in &frame.widgets {
            if matches!(
                widget.content,
                WidgetContent::Tabs(_) | WidgetContent::Status { .. }
            ) {
                for y in widget.area.top()..widget.area.bottom() {
                    for x in widget.area.left()..widget.area.right() {
                        assert_eq!(frame.buffer[(x, y)].symbol.as_str(), " ");
                    }
                }
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn graphical_input_edits_undoes_and_flushes_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unicode.txt");
        let (mut app, _) = app(Some(&path));
        let mut events = keys("ihello 界<esc>");
        events.extend([
            command("select_all"),
            command("delete_selection"),
            command("undo"),
            command(":write"),
        ]);
        app.run_frontend(&mut tokio_stream::iter(events))
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello 界\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn picker_publishes_semantic_rows_without_rasterizing_and_activates_by_id() {
        use helix_term::frontend::{UiEvent, WidgetContent};
        let (mut app, rx) = app(None);
        app.run_frontend(&mut tokio_stream::empty()).await.unwrap();
        let before = rx.borrow().clone();
        app.run_frontend(&mut tokio_stream::iter([command("buffer_picker")]))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let widget = frame
            .widgets
            .iter()
            .find(|widget| matches!(widget.content, WidgetContent::Picker(_)))
            .unwrap();
        let WidgetContent::Picker(picker) = &widget.content else {
            panic!("expected semantic picker");
        };
        assert!(!picker.rows.is_empty());
        assert!(!picker.rows[0].columns.is_empty());
        assert!(frame.buffer.lists.is_empty());
        // The picker neither writes text nor ASCII borders into the fallback.
        for y in widget.area.top()..widget.area.bottom() {
            for x in widget.area.left()..widget.area.right() {
                assert_eq!(frame.buffer[(x, y)], before.buffer[(x, y)]);
            }
        }
        let event = ApplicationEvent::Ui(UiEvent::ActivatePicker {
            id: picker.id,
            index: picker.rows[0].index,
        });
        app.run_frontend(&mut tokio_stream::iter([event]))
            .await
            .unwrap();
        assert!(!rx
            .borrow()
            .widgets
            .iter()
            .any(|widget| matches!(widget.content, WidgetContent::Picker(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stale_picker_click_cannot_activate_a_different_query() {
        use helix_term::frontend::{UiEvent, WidgetContent};
        let (mut app, rx) = app(None);
        app.run_frontend(&mut tokio_stream::iter([command("buffer_picker")]))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let WidgetContent::Picker(picker) = &frame
            .widgets
            .iter()
            .find(|widget| widget.has_input())
            .unwrap()
            .content
        else {
            panic!();
        };
        let id = picker.id;
        let mut events = keys("no-such-buffer-xyz");
        events.push(ApplicationEvent::Ui(UiEvent::ActivatePicker {
            id,
            index: 0,
        }));
        app.run_frontend(&mut tokio_stream::iter(events))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let WidgetContent::Picker(picker) = &frame
            .widgets
            .iter()
            .find(|widget| widget.has_input())
            .unwrap()
            .content
        else {
            panic!();
        };
        assert_eq!(picker.prompt.text, "no-such-buffer-xyz");
        assert_ne!(picker.id, id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_prompt_preserves_unicode_cursor_and_completes_by_id() {
        use helix_term::frontend::{UiEvent, WidgetContent};
        let (mut app, rx) = app(None);
        app.run_frontend(&mut tokio_stream::iter(keys(":echo 界😀<left>")))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let WidgetContent::Prompt(prompt) = &frame
            .widgets
            .iter()
            .find(|widget| widget.has_input())
            .unwrap()
            .content
        else {
            panic!();
        };
        assert_eq!(prompt.text, "echo 界😀");
        assert_eq!(prompt.cursor, "echo 界".len());
        // Prompt text is exclusively in the semantic model.
        assert!(!frame
            .buffer
            .content
            .iter()
            .any(|cell| cell.symbol.contains('界')));

        app.run_frontend(&mut tokio_stream::iter(keys("<esc>:wri")))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let WidgetContent::Prompt(prompt) = &frame
            .widgets
            .iter()
            .find(|widget| widget.has_input())
            .unwrap()
            .content
        else {
            panic!();
        };
        let index = prompt
            .completions
            .iter()
            .position(|text| {
                text.0
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
                    == "write"
            })
            .unwrap();
        let event = ApplicationEvent::Ui(UiEvent::CompletePrompt {
            id: prompt.id,
            index,
        });
        app.run_frontend(&mut tokio_stream::iter([event]))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let WidgetContent::Prompt(prompt) = &frame
            .widgets
            .iter()
            .find(|widget| widget.has_input())
            .unwrap()
            .content
        else {
            panic!();
        };
        assert_eq!(prompt.text, "write");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn modified_buffer_rejects_window_quit() {
        let (mut app, _) = app(None);
        let mut events = keys("ichanged<esc>");
        events.push(command(":quit-all"));
        app.run_frontend(&mut tokio_stream::iter(events))
            .await
            .unwrap();
        assert!(!app.editor.should_close());
        assert!(app.editor.status_msg.is_some());
    }
}
