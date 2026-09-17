#!/usr/bin/env bash
# Run from the workspace root after `cargo build -p helix-gpui`.
# Set HELIX_GPUI_BIN to test an installed binary with its packaged runtime.
# Requires Xvfb, openbox, xdotool and ImageMagick. Uses an isolated X server.
set -euo pipefail
for program in Xvfb openbox xdotool import magick; do
  command -v "$program" >/dev/null || { echo "Missing $program" >&2; exit 1; }
done
smoke_dir=$(mktemp -d /tmp/helix-gpui-smoke.XXXXXX)
export XDG_CONFIG_HOME="$smoke_dir/config"
if [[ -z ${HELIX_GPUI_BIN:-} ]]; then
  export HELIX_RUNTIME="${HELIX_RUNTIME:-$PWD/runtime}"
fi
export HELIX_GPUI_TRACE_LATENCY=1
unset WAYLAND_DISPLAY
Xvfb -displayfd 3 -screen 0 1280x900x24 3>"$smoke_dir/display" >"$smoke_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
trap 'kill "$xvfb_pid" ${wm_pid:-} ${app_pid:-} 2>/dev/null || true' EXIT
for attempt in $(seq 1 50); do
  test -s "$smoke_dir/display" && break
  sleep 0.1
done
export DISPLAY=":$(cat "$smoke_dir/display")"
openbox >"$smoke_dir/wm.log" 2>&1 &
wm_pid=$!
"${HELIX_GPUI_BIN:-target/debug/hx-gpui}" --log "$smoke_dir/helix.log" "$smoke_dir/example.txt" >"$smoke_dir/app.log" 2>&1 &
app_pid=$!
window_id=$(timeout 30 xdotool search --sync --onlyvisible --name '^Helix$' | head -1)
xdotool windowactivate --sync "$window_id"
sleep 2
xdotool type --clearmodifiers --delay 30 'iHello from GPUI'
xdotool key Escape
xdotool key ctrl+shift+s
sleep 0.5
test "$(cat "$smoke_dir/example.txt")" = 'Hello from GPUI'
import -window "$window_id" "$smoke_dir/buffer.png"

# Let the background word index settle, then accept an insert-mode completion
# with a real click and verify the resulting edit before undoing it.
sleep 1.5
xdotool type --clearmodifiers 'oHel'
sleep 1.5
xdotool key ctrl+x
sleep 0.5
import -window "$window_id" "$smoke_dir/completion.png"
xdotool mousemove --window "$window_id" 128 108 click 1
xdotool key Escape ctrl+shift+s
sleep 0.3
test "$(cat "$smoke_dir/example.txt")" = "$(printf 'Hello from GPUI\nHello')"
xdotool key u
sleep 0.15
xdotool key ctrl+shift+s
sleep 0.3
test "$(cat "$smoke_dir/example.txt")" = "$(printf 'Hello from GPUI\nHel')"
xdotool key u
sleep 0.15
xdotool key ctrl+shift+s
sleep 0.3
test "$(cat "$smoke_dir/example.txt")" = 'Hello from GPUI'

# Semantic picker, rendered as GPUI controls, accepts a real pointer click.
xdotool key space b
sleep 0.5
import -window "$window_id" "$smoke_dir/picker.png"
xdotool mousemove --window "$window_id" 150 182 click 1
sleep 0.5
# Save is available directly without a menu bar.
xdotool key ctrl+shift+s
sleep 0.5

# Command input and completions use native controls, including the prompt caret.
xdotool type --clearmodifiers --delay 40 ':wri'
sleep 0.5
import -window "$window_id" "$smoke_dir/prompt.png"
xdotool key Tab Return
sleep 0.5

# Native clipboard commands are serviced while the editor thread waits.
xdotool type --clearmodifiers '%'
xdotool key space y
xdotool key d
xdotool key space shift+p
sleep 0.5
xdotool key ctrl+shift+s
sleep 0.5
test "$(cat "$smoke_dir/example.txt")" = 'Hello from GPUI'

# Key hints are native rows with key labels and descriptions.
xdotool key space
sleep 0.3
import -window "$window_id" "$smoke_dir/key-hints.png"
xdotool key Escape

# Open a second document and switch back using a real tab click.
xdotool type --clearmodifiers --delay 5 ":open $smoke_dir/second.txt"
xdotool key Return
sleep 0.3
xdotool type --clearmodifiers 'iSecond document'
xdotool key Escape ctrl+shift+s
sleep 0.3
test "$(cat "$smoke_dir/second.txt")" = 'Second document'
xdotool mousemove --window "$window_id" 90 28 click 1
xdotool type --clearmodifiers 'A!'
xdotool key Escape ctrl+shift+s
sleep 0.3
test "$(cat "$smoke_dir/example.txt")" = 'Hello from GPUI!'
xdotool key u
sleep 0.15
xdotool key ctrl+shift+s
sleep 0.3
import -window "$window_id" "$smoke_dir/tabs.png"

# Drag an actual native split divider, identified by its solid border color.
xdotool type --clearmodifiers ':vsplit'
xdotool key Return
sleep 0.3
import -window "$window_id" "$smoke_dir/split.png"
split_x=$(magick "$smoke_dir/split.png" -crop 1100x1+0+180 txt:- | awk -F'[:,]' '/#303034/ && !found {print $1; found=1}')
test -n "$split_x"
xdotool mousemove --window "$window_id" "$((split_x + 2))" 180 mousedown 1
xdotool mousemove --window "$window_id" "$((split_x + 150))" 180
sleep 0.3
xdotool mouseup 1
sleep 0.3
import -window "$window_id" "$smoke_dir/split-dragged.png"
dragged_x=$(magick "$smoke_dir/split-dragged.png" -crop 1100x1+0+180 txt:- | awk -F'[:,]' '/#303034/ && !found {print $1; found=1}')
test -n "$dragged_x"
test "$dragged_x" -gt "$((split_x + 100))"
xdotool type --clearmodifiers ':quit'
xdotool key Return
sleep 0.3

# Horizontal dividers use the same drag behavior with vertical coordinates.
xdotool type --clearmodifiers ':hsplit'
xdotool key Return
sleep 0.3
import -window "$window_id" "$smoke_dir/horizontal-split.png"
split_y=$(magick "$smoke_dir/horizontal-split.png" -crop 1x760+300+0 txt:- | awk -F'[:,]' '/#303034/ && $2 > 80 && !found {print $2; found=1}')
test -n "$split_y"
xdotool mousemove --window "$window_id" 300 "$((split_y + 2))" mousedown 1
xdotool mousemove --window "$window_id" 300 "$((split_y + 100))"
sleep 0.3
xdotool mouseup 1
sleep 0.3
import -window "$window_id" "$smoke_dir/horizontal-dragged.png"
dragged_y=$(magick "$smoke_dir/horizontal-dragged.png" -crop 1x760+300+0 txt:- | awk -F'[:,]' '/#303034/ && $2 > 80 && !found {print $2; found=1}')
test -n "$dragged_y"
test "$dragged_y" -gt "$((split_y + 70))"
xdotool type --clearmodifiers ':quit'
xdotool key Return
sleep 0.3

# An unsaved edit must keep the window alive after a desktop quit request.
xdotool type --clearmodifiers 'A!'
xdotool key Escape ctrl+shift+q
sleep 0.5
kill -0 "$app_pid"
xdotool key u
sleep 0.15
xdotool key ctrl+shift+s
sleep 0.5
# Resize is processed by the editor before publishing the next frame.
xdotool windowsize "$window_id" 800 600
sleep 0.5
import -window "$window_id" "$smoke_dir/resized.png"
xdotool key ctrl+shift+q
for attempt in $(seq 1 50); do
  if ! kill -0 "$app_pid" 2>/dev/null; then break; fi
  sleep 0.1
done
if kill -0 "$app_pid" 2>/dev/null; then
  echo "Application did not quit; see $smoke_dir" >&2
  exit 1
fi
wait "$app_pid"
test "$(cat "$smoke_dir/example.txt")" = 'Hello from GPUI'
printf 'Smoke test passed. Artifacts: %s\n' "$smoke_dir"
