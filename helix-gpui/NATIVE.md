# Native GPUI rendering

The initial hybrid frontend is checkpointed at `6a3dba0e`.

## Rendering boundary

Only document buffers and document previews use the cell-based buffer element.
This preserves Helix's existing selection, syntax, gutter, soft-wrap and annotation
behavior. Previews have separate buffer elements so they compose correctly above
tabs and other editor chrome.

Every other component implements `Component::render_native` and publishes semantic
data from its model. That method has no default TUI fallback. The graphical path
does not rasterize menus, frames, status lines or tabs into buffer cells.

GPUI controls include:

- Clickable document tabs, close buttons and modified indicators. Closing an
  unsaved tab reports an error and keeps the document open.
- Mouse-draggable vertical and horizontal split dividers. The editor's view tree
  stores proportions, retains neighboring sizes, and clamps minimum pane sizes.
- Status bars, messages, counts and recording/trust indicators.
- Picker panels, headers, styled rows, fuzzy highlights, query input and counts.
- Command/search prompts, history suggestions, clickable completions, syntax
  coloring, Unicode caret hit testing and IME composition display.
- Clickable key-combination hints, completion/code-action menus, selection
  dialogs and debugger menus.
- Hover and signature documentation, with native Markdown headings, code blocks,
  rules and clickable links. Help panels support pointer and keyboard scrolling.

The semantic types live in `helix-term/src/frontend.rs`; the engine has no GPUI
dependency. GPUI elements live in `helix-gpui/src/widgets.rs`. Interaction tokens
reject actions from obsolete queries. Nested overlays and popups forward logical
actions to their contents, preserving the existing Helix callbacks and commands.
The separate `hx` executable keeps its terminal renderer.

## Input latency

The graphical input loop never waits on the terminal render lock held by background
Git diff work. A completed diff requests a subsequent redraw. Input events keep
their original order; redraws within an 8 ms interval are combined, preventing a
burst of key events from queuing a full frame per key. The buffer element also
skips shaping blank text runs.

Set `HELIX_GPUI_TRACE_LATENCY=1` to log `input_to_paint_ms` measurements in the Helix
log. They measure frontend event delivery through the start of GPUI painting,
including the editor thread and frame scheduling, rather than physical display
scanout. Run `bash helix-gpui/tests/smoke-x11.sh` from the workspace root to
exercise real input, tabs, split dragging, prompts, menus, clipboard and shutdown.

Tests also hold a background render lock while editing, exercise input bursts,
check split proportions, activate native popup rows, and verify that native chrome
does not write into the buffer cell surface. Platform accessibility and
surrounding-document IME reconversion remain separate follow-up work; interactive
validation currently covers Linux/X11.
