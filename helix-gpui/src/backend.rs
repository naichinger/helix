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
    pub cursor: (u16, u16),
    pub cursor_kind: CursorKind,
    pub background: Color,
    pub exit: Option<Result<i32, String>>,
}

impl Frame {
    pub fn new(area: Rect) -> Self {
        Self {
            buffer: Buffer::empty(area),
            cursor: (0, 0),
            cursor_kind: CursorKind::Hidden,
            background: Color::Reset,
            exit: None,
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
        let backend = SurfaceBackend::new(area, tx);
        let config = Config::default();
        let trust = helix_loader::workspace_trust::WorkspaceTrust::new(
            (&config.editor.workspace_trust).into(),
        );
        let mut args = Args::default();
        if let Some(path) = path {
            args.files
                .insert(path.into(), vec![helix_core::Position::new(0, 0)]);
        }
        (
            Application::new_with_backend(
                args,
                config,
                helix_core::config::default_lang_loader(),
                trust,
                backend,
                false,
            )
            .unwrap(),
            rx,
        )
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
    async fn picker_publishes_native_list_and_accepts_mouse() {
        let (mut app, rx) = app(None);
        let events = vec![command("buffer_picker")];
        app.run_frontend(&mut tokio_stream::iter(events))
            .await
            .unwrap();
        let frame = rx.borrow().clone();
        let area = frame.buffer.lists[0];
        assert!(area.width > 0 && area.height > 0);
        app.handle_input_event(Event::Mouse(helix_view::input::MouseEvent {
            kind: helix_view::input::MouseEventKind::Down(helix_view::input::MouseButton::Left),
            column: area.x,
            row: area.y,
            modifiers: helix_view::input::KeyModifiers::empty(),
        }))
        .await;
        assert!(rx.borrow().buffer.lists.is_empty());
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
