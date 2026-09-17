#!/usr/bin/env bash
# Run from the workspace root after `cargo build -p helix-gpui`.
# Requires Xvfb, openbox, xdotool and ImageMagick. Uses an isolated X server.
set -euo pipefail
for program in Xvfb openbox xdotool import; do
  command -v "$program" >/dev/null || { echo "Missing $program" >&2; exit 1; }
done
smoke_dir=$(mktemp -d /tmp/helix-gpui-smoke.XXXXXX)
export XDG_CONFIG_HOME="$smoke_dir/config"
export HELIX_RUNTIME="${HELIX_RUNTIME:-$PWD/runtime}"
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
target/debug/hx-gpui --log "$smoke_dir/helix.log" "$smoke_dir/example.txt" >"$smoke_dir/app.log" 2>&1 &
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

# Shared picker, rendered as a GPUI uniform_list, accepts a real pointer click.
xdotool key space b
sleep 0.5
import -window "$window_id" "$smoke_dir/picker.png"
xdotool mousemove --window "$window_id" 150 122 click 1
sleep 0.5
# Menu keyboard navigation: File > Save.
xdotool key F10 Down Down Down Return
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

# An unsaved edit must keep the window alive after a desktop quit request.
xdotool type --clearmodifiers 'A!'
xdotool key Escape ctrl+shift+q
sleep 0.5
kill -0 "$app_pid"
xdotool key u ctrl+shift+s
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
