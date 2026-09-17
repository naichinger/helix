# Helix GPUI frontend

`hx-gpui` opens Helix in a GPU-rendered desktop window. The terminal executable
remains `hx` and remains the default workspace target.

```sh
cargo run -p helix-gpui -- path/to/file.rs
cargo run -p helix-gpui -- --vsplit first.rs second.rs
cargo run -p helix-gpui -- --tutor
```

With Nix, from the repository root:

```sh
nix run
nix run . -- README.md
nix run .#helix -- README.md # terminal frontend
```

The default flake application is GPUI. Its package includes the Helix runtime,
tree-sitter grammars, and Linux library paths needed to start without a development
shell. `nix develop` provides the GPUI build environment.

The GPUI version is pinned to 0.2.2. On Linux, building requires Clang/libclang,
pkg-config, OpenSSL, Fontconfig, FreeType, Wayland, X11/XCB, libxkbcommon (including
its X11 library), and Vulkan development libraries. A Vulkan-capable driver is
required at runtime. The CI dependency setup is in
[gpui-setup](../.github/actions/gpui-setup/action.yml).

Helix's normal build script fetches and builds tree-sitter grammars. For a quick
frontend build with existing grammars, set `HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1`.
If grammars have not been built, syntax highlighting, structural motions and
syntax-aware indentation will be unavailable. Build them with:

```sh
cargo run -p helix-gpui -- --grammar fetch
cargo run -p helix-gpui -- --grammar build
```

`HELIX_RUNTIME` can point to this checkout's `runtime` directory when launching
an installed binary. Config, themes, language configuration, workspace trust,
keymaps, language servers and debug adapters use the same locations as `hx`.
The frontend accepts Helix's command-line options, including `file:line:column`,
`--working-dir`, `--config`, `--log`, `--health`, and split options. It starts a
scratch buffer when no files are passed and does not read stdin.

## Editing and desktop controls

Helix commands, modal editing, multiple selections, registers, macros, undo/redo,
search, splits, pickers, syntax highlighting, diagnostics, completion, LSP actions,
DAP debugging, Git gutters, shell commands, formatting, configuration reload and
save operations run through the shared Helix application and event loop.
Language servers, debug adapters and Git features retain their normal setup and
workspace-trust requirements.

- Type and navigate with your Helix keymaps. Special keys and modifiers preserve
  Helix semantics; printable text comes through GPUI's platform input handler.
- Click or drag in a buffer to position or extend selections. Wheel and trackpad
  input scroll the view under the pointer. Click picker or completion rows to
  select them. Keyboard filtering and navigation work as in `hx`.
- Click tabs to switch documents and their close buttons to close saved documents.
  Modified documents remain open with a save reminder. Drag the divider between
  splits to resize either vertically or horizontally; proportions survive window
  resizing.
- Drop files or folders into the window, use the open-file shortcut, or use Helix's picker.
- The window has no top menu bar. Use the command palette, Helix keymaps, or
  command prompt for editor actions. The tab strip includes a new-buffer button.
- On Linux/Windows, Ctrl+Shift+O opens files, Ctrl+Shift+S saves,
  Ctrl+Shift+P opens the command palette, and Ctrl+Shift+Q quits.
  On macOS, use Command with those letters.
- Clipboard commands and the `+` register use GPUI's system clipboard. Linux's
  primary selection (`*`) uses GPUI's primary-selection APIs. Platforms without
  a primary selection retain a separate local register.
- Closing the window requests `:quit-all`. Modified buffers keep the window open
  and Helix displays its normal error. Save first or explicitly use `:quit-all!`
  to discard modifications. Pending writes and language servers are closed
  before the UI exits.
- Ctrl+Shift+=/−/0 (Command on macOS) zooms in/out or resets the buffer font size. `HELIX_GPUI_FONT` selects an installed
  font family; `HELIX_GPUI_FONT_SIZE` sets the initial size (8–40 pixels).
  The default selects an installed monospace font. Adaptive Helix themes follow
  platform appearance changes.

## Appearance

The desktop shell takes inspiration from Zeron: neutral layered surfaces, a
sans-serif UI font, rounded tabs and panels, muted labels, thin dividers and a
restrained violet accent. The command prompt floats at the top center, and the
status bar fills the bottom edge without reserving an empty message row. Buffer
text keeps its configured monospace font and syntax colors.

When no theme is configured, `hx-gpui` uses `helix_gpui`. Explicit Helix themes are
respected, and the chrome switches between light and dark neutral palettes to
match the buffer. You can also select the new theme with `:theme helix_gpui`.
UI tokens live in `src/design.rs`; the editor palette lives in
`runtime/themes/helix_gpui.toml`.

For Zeron's diff-viewer colors, use `:theme zeron_diff` or set
`theme = "zeron_diff"` in your Helix configuration. This theme uses Zeron's
near-black background, violet keywords, blue functions, pink properties, emerald
strings, and faint green/red diff backgrounds.

## Rendering and integration

The buffer uses a custom GPUI `Element`, shaped text runs, GPU quads, content
clipping, and `ElementInputHandler`. It maintains Helix's grapheme-column geometry
for selections, soft wrap, gutters, splits, annotations, cursor positions and hit
testing. Adjacent text with identical styling is shaped in runs.

Pickers and command/search prompts have a separate semantic rendering path.
Helix publishes full text, styled columns, fuzzy-match highlights, query state,
completion candidates and logical row identities. GPUI builds bordered panels,
`StyledText` labels, `uniform_list` rows and shaped input fields from those models.
These components do not render terminal cells or ASCII borders. Pointer actions
identify the selected item directly; stale actions after a query change are ignored.
Prompt caret placement and horizontal scrolling use GPUI text shaping, including
Unicode and IME preedit. The old code that scraped picker rows from cells is gone.

Desktop shortcuts use GPUI actions and execute commands directly through Helix's command
state, preserving undo history and pending-input handling with customized keymaps.
Tabs, split dividers, status bars, key hints, completion/code-action menus, hover,
signature help, Markdown and dialogs all use native GPUI elements. Markdown rules
are graphical dividers and links are clickable. Only document buffers and their
previews use the cell-based buffer element. Every component must provide a native
renderer; there is no TUI fallback for editor chrome. See [native rendering](NATIVE.md).
There is no PTY or terminal escape-sequence parser.

The compositor stays on a dedicated thread under Tokio. GPUI receives coalesced
immutable frames, and sends input back through a channel. The same asynchronous
loop processes editor, LSP, DAP and job events. Resizing takes effect on that
thread before layout. Clipboard requests are serviced on GPUI's foreground
thread. Frontend failures are displayed and logged.

Input processing does not wait for background diff rendering locks. Redraws are
combined within an 8 ms interval while retaining every input event in order.
`HELIX_GPUI_TRACE_LATENCY=1` enables input-to-paint timings in the Helix log.

## Validation and current limits

```sh
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo test -p helix-gpui
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo test -p helix-term --features integration --test integration
bash helix-gpui/tests/smoke-x11.sh
```

The frontend tests cover buffer frames and resizing, modifier translation, shortcut
command validity, Unicode editing and saving, undo after desktop actions, semantic
picker activation, stale-action rejection, Unicode prompt cursors, prompt completion,
and modified-buffer quit protection. They also verify that native pickers and
prompts do not rasterize their contents into the buffer surface.
Additional tests cover native tab switching and close protection, popup callbacks,
Markdown links/rules, split resizing, rapid input and background-render-lock latency.
The X11 smoke test requires Xvfb, Openbox, xdotool and ImageMagick, and runs in
an isolated display. It exercises real keyboard input, desktop shortcuts, picker and
insert-completion clicks, tab switching, divider dragging, clipboard round trips,
saving, resizing and quit protection. Screenshots are saved in its temporary
artifact directory.

Linux/X11 has been exercised with a real GPUI window, keyboard input, saving,
pickers and shutdown. Wayland, macOS and Windows still need interactive validation.
IME composition is supported at the cursor; surrounding-document reconversion
and platform text replacement outside the composition are not implemented.
The buffer does not yet expose a document accessibility tree. The editor uses
one OS window with Helix's internal splits. These limits mean this should not yet
be described as a fully validated, feature-complete desktop frontend.
