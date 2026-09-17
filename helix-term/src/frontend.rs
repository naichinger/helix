//! Semantic UI data for graphical frontends. Text here comes from component
//! models, before terminal layout, truncation, borders, or cell rasterization.
use helix_view::graphics::{CursorKind, Rect, Style};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Default)]
pub struct RichText(pub Vec<TextRun>);

#[derive(Clone, Debug)]
pub struct TextRun {
    pub text: String,
    pub style: Style,
}

impl RichText {
    pub fn plain(text: impl Into<String>, style: Style) -> Self {
        Self(vec![TextRun {
            text: text.into(),
            style,
        }])
    }

    pub fn from_text(text: &tui::text::Text<'_>) -> Self {
        let mut runs = Vec::new();
        for (i, line) in text.lines.iter().enumerate() {
            if i != 0 {
                runs.push(TextRun {
                    text: "\n".into(),
                    style: Style::default(),
                });
            }
            runs.extend(line.0.iter().map(|span| TextRun {
                text: span.content.to_string(),
                style: span.style,
            }));
        }
        Self(runs)
    }
}

/// A short-lived interaction token. Components discard actions from a UI
/// snapshot they no longer display (for example after a picker query changes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetId(u64);

impl WidgetId {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    ActivatePicker {
        id: WidgetId,
        index: u32,
    },
    CompletePrompt {
        id: WidgetId,
        index: usize,
    },
    ScrollPicker {
        id: WidgetId,
        lines: i32,
    },
    Menu {
        id: WidgetId,
        index: usize,
        accept: bool,
    },
    ActivateTab(helix_view::DocumentId),
    CloseTab(helix_view::DocumentId),
    ResizeSplit {
        parent: helix_view::ViewId,
        index: usize,
        position: u16,
    },
    Keys(Vec<helix_view::input::KeyEvent>),
    PromptCursor {
        id: WidgetId,
        byte: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: helix_view::DocumentId,
    pub label: String,
    pub path: String,
    pub modified: bool,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub id: WidgetId,
    pub rows: Vec<Vec<RichText>>,
    pub selected: Option<usize>,
    pub offset: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Default)]
pub enum BlockKind {
    #[default]
    Paragraph,
    Heading,
    Code,
    Quote,
    Item(String),
    Rule,
}

#[derive(Clone, Debug, Default)]
pub struct TextBlock {
    pub kind: BlockKind,
    pub text: RichText,
    pub links: Vec<(std::ops::Range<usize>, String)>,
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub id: WidgetId,
    pub label: String,
    pub text: String,
    /// UTF-8 byte offset, always at a character boundary.
    pub cursor: usize,
    pub suggestion: Option<String>,
    pub completions: Vec<RichText>,
    pub selected: Option<usize>,
    pub documentation: Option<String>,
    pub styled_text: RichText,
}

#[derive(Clone, Debug)]
pub struct PickerRow {
    pub index: u32,
    pub columns: Vec<RichText>,
}

#[derive(Clone, Debug)]
pub struct Picker {
    pub id: WidgetId,
    pub prompt: Prompt,
    pub headers: Vec<RichText>,
    pub widths: Vec<usize>,
    pub rows: Vec<PickerRow>,
    pub selected: u32,
    pub matched: u32,
    pub total: u32,
    pub running: bool,
}

#[derive(Clone, Debug)]
pub enum WidgetContent {
    Picker(Picker),
    Prompt(Prompt),
    Menu(Menu),
    Text {
        title: String,
        text: RichText,
    },
    Hints {
        title: String,
        entries: Vec<(String, String)>,
    },
    Tabs(Vec<Tab>),
    Status {
        left: RichText,
        center: RichText,
        right: RichText,
    },
    Divider(helix_view::tree::SplitDivider),
    Border,
    Buffer(std::sync::Arc<tui::buffer::Buffer>),
    Document {
        title: String,
        blocks: Vec<TextBlock>,
    },
}

#[derive(Clone, Debug)]
pub struct Widget {
    pub scroll: usize,
    /// Placement in editor layout units. The graphical frontend lays out the
    /// contents itself; these are not terminal cells or strings of padding.
    pub area: Rect,
    pub style: Style,
    pub selected_style: Style,
    pub border_style: Style,
    pub content: WidgetContent,
}

impl Widget {
    pub fn new(area: Rect, style: Style, content: WidgetContent) -> Self {
        Self {
            scroll: 0,
            area,
            style,
            selected_style: style,
            border_style: style,
            content,
        }
    }

    pub fn has_input(&self) -> bool {
        matches!(
            self.content,
            WidgetContent::Picker(_) | WidgetContent::Prompt(_)
        )
    }
}

/// Only document buffers (including previews) draw into the cell surface.
/// All editor chrome is represented by semantic widgets.
pub struct Frame<'a> {
    pub buffer: &'a tui::buffer::Buffer,
    pub widgets: Vec<Widget>,
    pub cursor: Option<helix_core::Position>,
    pub cursor_kind: CursorKind,
    pub background: Style,
    pub input_at: Option<std::time::Instant>,
}

pub type Renderer = Box<dyn FnMut(Frame<'_>)>;
