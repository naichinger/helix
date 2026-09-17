# Helix GPUI frontend

`hx-gpui` opens Helix in a GPU-rendered desktop window. The terminal executable
remains `hx` and remains the default workspace target.

```sh
cargo run -p helix-gpui -- path/to/file.rs
cargo run -p helix-gpui -- --vsplit first.rs second.rs
cargo run -p helix-gpui -- --tutor
```

On NixOS, from the repository root:

```sh
nix-shell helix-gpui/shell.nix
cargo run -p helix-gpui -- README.md
```

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
- Drop files or folders into the window, use File → Open, or use Helix's picker.
- Use the in-window File, Edit, Selection, Search, View, Code and Debug menus.
  GPUI also installs the platform menu bar where supported. F10 focuses the
  in-window menus; arrows navigate, Enter activates, and Escape dismisses.
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
- View → Zoom changes the buffer font size. `HELIX_GPUI_FONT` selects an installed
  font family; `HELIX_GPUI_FONT_SIZE` sets the initial size (8–40 pixels).
  The default selects an installed monospace font. Adaptive Helix themes follow
  platform appearance changes.

## Rendering and integration

The buffer uses a custom GPUI `Element`, shaped text runs, GPU quads, content
clipping, and `ElementInputHandler`. It maintains Helix's grapheme-column geometry
for selections, soft wrap, gutters, splits, annotations, cursor positions and hit
testing. Adjacent text with identical styling is shaped in runs.

Picker and completion/menu regions are identified during Helix layout and rendered
with GPUI `uniform_list` and interactive row elements. Menus use GPUI actions;
they execute commands directly through Helix's command state, preserving undo
history and pending-input handling even with customized keymaps. Other Helix
surfaces (prompts, statuslines, borders, documentation and previews) use the
shared styled surface. This is an in-process frontend, with no PTY or terminal
escape-sequence parser.

The compositor stays on a dedicated thread under Tokio. GPUI receives coalesced
immutable frames, and sends input back through a channel. The same asynchronous
loop processes editor, LSP, DAP and job events. Resizing takes effect on that
thread before layout. Clipboard requests are serviced on GPUI's foreground
thread. Frontend failures are displayed and logged.

## Validation and current limits

```sh
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo test -p helix-gpui
HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo test -p helix-term --features integration --test integration
bash helix-gpui/tests/smoke-x11.sh
```

The frontend tests cover buffer frames and resizing, modifier translation, menu
command validity, Unicode editing and saving, undo after menu actions, picker
mouse activation, and modified-buffer quit protection.
The X11 smoke test requires Xvfb, Openbox, xdotool and ImageMagick, and runs in
an isolated display. It exercises real keyboard input, native menus, picker
clicks, clipboard round trips, saving, resizing and quit protection; screenshots
are saved in its temporary artifact directory.

Linux/X11 has been exercised with a real GPUI window, keyboard input, saving,
pickers and shutdown. Wayland, macOS and Windows still need interactive validation.
IME composition is supported at the cursor; surrounding-document reconversion
and platform text replacement outside the composition are not implemented.
The buffer does not yet expose a document accessibility tree. The editor uses
one OS window with Helix's internal splits. These limits mean this should not yet
be described as a fully validated, feature-complete desktop frontend.
