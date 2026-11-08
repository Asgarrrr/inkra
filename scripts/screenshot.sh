#!/usr/bin/env bash
# Captures the README screenshot: one note in a centred window, over a chosen wallpaper.
# usage: scripts/screenshot.sh --vault DIR --note PATH [--wallpaper IMAGE] [--size 1200x760] [--padding 80] [--settle 3] [--out docs/assets/screenshot.webp]
# PATH is vault-relative. The wallpaper, the Dock, the desktop icons and the hidden apps are restored on exit.
# Needs Accessibility and Screen Recording for the terminal, in System Settings > Privacy & Security.
# A .webp output is lossless, half the PNG's size; it needs cwebp (brew install webp).
set -euo pipefail
cd "$(dirname "$0")/.."

vault= note= wallpaper= size=1200x760 padding=80 settle=3 out=docs/assets/screenshot.webp
while (($#)); do
  case $1 in
    --vault) vault=$2; shift 2 ;;
    --note) note=$2; shift 2 ;;
    --wallpaper) wallpaper=$2; shift 2 ;;
    --size) size=$2; shift 2 ;;
    --padding) padding=$2; shift 2 ;;
    --settle) settle=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    *) echo "usage: see the header of $0" >&2; exit 2 ;;
  esac
done
[[ -n $vault && -n $note ]] || { echo "--vault and --note are required" >&2; exit 2; }
root=$(cd "$vault" && pwd)
[[ -f $root/$note ]] || { echo "no note $note in $root" >&2; exit 1; }
[[ -z $wallpaper || -f $wallpaper ]] || { echo "no image $wallpaper" >&2; exit 1; }
[[ $out != *.webp ]] || command -v cwebp > /dev/null || { echo "a .webp output needs cwebp: brew install webp" >&2; exit 1; }
[[ -n $wallpaper ]] && wallpaper=$(cd "$(dirname "$wallpaper")" && pwd)/$(basename "$wallpaper")
width=${size%x*} height=${size#*x}

# The app's session file name: FNV-1a 64 of the root's bytes, as in crates/app/src/app_files.rs.
fnv1a() {
  local LC_ALL=C s=$1 h=$((0xcbf29ce484222325)) i c
  for ((i = 0; i < ${#s}; i++)); do
    printf -v c '%d' "'${s:i:1}"
    h=$(((h ^ (c & 255)) * 0x100000001b3))
  done
  printf '%016x' "$h"
}

cargo build --release -q -p inkra-app

# A throwaway HOME: the note opens from a written session, and the real session stays untouched.
home=$(mktemp -d)
sessions="$home/Library/Application Support/Inkra/sessions"
mkdir -p "$sessions"
json() { sed 's/\\/\\\\/g; s/"/\\"/g' <<< "$1"; }
cat > "$sessions/$(fnv1a "$root").json" << EOF
{
  "version": 3,
  "root": "$(json "$root")",
  "active": 0,
  "center": { "tabs": [{ "file": "$(json "$note")" }], "active": 0 },
  "left": { "tabs": [{ "view": "files" }], "active": 0 },
  "right": { "tabs": [{ "view": "backlinks" }], "active": 0 },
  "shown": [true, true]
}
EOF

applescript() { osascript - "$@" > /dev/null; }
# No argument: prints each desktop's wallpaper. Arguments: sets them, the last repeated on any desktop left.
# On macOS 26 a set needs both: NSWorkspace writes the current Space's wallpaper but leaves the screen unchanged,
# System Events shows the picture at once, and the Space's own wallpaper takes over again within seconds.
wallpapers() {
  if (($#)); then
    osascript -l JavaScript - "$@" > /dev/null << 'EOF'
ObjC.import('AppKit')
function run(argv) {
  const workspace = $.NSWorkspace.sharedWorkspace, screens = $.NSScreen.screens
  for (let i = 0; i < screens.count; i++) {
    const path = argv[Math.min(i, argv.length - 1)]
    if (!workspace.setDesktopImageURLForScreenOptionsError($.NSURL.fileURLWithPath(path), screens.objectAtIndex(i), $({}), null)) throw new Error(`cannot set ${path}`)
  }
}
EOF
  fi
  osascript - "$@" << 'EOF'
on run argv
  tell application "System Events"
    set n to count of desktops
    if (count of argv) is 0 then
      set paths to {}
      repeat with i from 1 to n
        set end of paths to picture of desktop i
      end repeat
      set AppleScript's text item delimiters to linefeed
      return paths as text
    end if
    repeat with i from 1 to n
      if i > (count of argv) then
        set picture of desktop i to item -1 of argv
      else
        set picture of desktop i to item i of argv
      end if
    end repeat
  end tell
  return ""
end run
EOF
}
# The new wallpaper shows up to a second after the set; waits for a band below the menu bar to change.
band() { screencapture -x -R "0,60,$screen_w,300" "$1"; }
await_wallpaper() {
  local i
  for ((i = 0; i < 100; i++)); do
    band "$home/now.png"
    cmp -s "$home/before.png" "$home/now.png" || return 0
    sleep 0.1
  done
  echo "the wallpaper did not change on screen within 10 s" >&2
  return 1
}
lines() { osascript -e "tell application \"System Events\" to set r to get $1" -e "set AppleScript's text item delimiters to linefeed" -e 'r as text'; }

old_autohide=$(defaults read com.apple.dock autohide 2> /dev/null || echo unset)
old_icons=$(defaults read com.apple.finder CreateDesktop 2> /dev/null || echo unset)
IFS=$'\n' read -d '' -ra old_visible < <(lines 'name of every process whose visible is true') || true
old_pictures=()
if [[ -n $wallpaper ]]; then
  IFS=$'\n' read -d '' -ra old_pictures < <(wallpapers) || true
  for picture in "${old_pictures[@]}"; do
    # macOS keeps showing a deleted wallpaper from its cache, but it could not be set back.
    [[ -f $picture ]] || { echo "the current wallpaper $picture is gone, so it could not be restored; set another first" >&2; exit 1; }
  done
fi
read -r screen_w screen_h < <(osascript -l JavaScript -e 'ObjC.import("AppKit"); const s = $.NSScreen.mainScreen.frame.size; `${s.width} ${s.height}`')
pid=

restore() {
  if [[ -n $pid ]]; then
    kill "$pid" 2> /dev/null || true
    wait "$pid" 2> /dev/null || true
  fi
  ((${#old_pictures[@]})) && { wallpapers "${old_pictures[@]}" > /dev/null || true; }
  if [[ $old_autohide == unset ]]; then
    defaults delete com.apple.dock autohide 2> /dev/null || true
  else
    defaults write com.apple.dock autohide -bool "$([[ $old_autohide == 1 ]] && echo true || echo false)"
  fi
  killall Dock 2> /dev/null || true
  if [[ $old_icons == unset ]]; then
    defaults delete com.apple.finder CreateDesktop 2> /dev/null || true
  else
    defaults write com.apple.finder CreateDesktop -bool "$([[ $old_icons == 1 ]] && echo true || echo false)"
  fi
  killall Finder 2> /dev/null || true
  applescript "${old_visible[@]}" << 'EOF' || true
on run argv
  tell application "System Events"
    repeat with name_ in argv
      if exists process name_ then set visible of process name_ to true
    end repeat
  end tell
end run
EOF
  rm -r "$home"
}
trap restore EXIT

defaults write com.apple.finder CreateDesktop -bool false
killall Finder
# Finder relaunches visible, so the apps hide after it is back.
sleep 1
applescript << 'EOF'
tell application "System Events"
  set visible of every process whose visible is true to false
end tell
EOF
# With the apps hidden, the band shows the wallpaper, not a window.
sleep 0.5
if [[ -n $wallpaper ]]; then
  band "$home/before.png"
  wallpapers "$wallpaper" > /dev/null
  await_wallpaper
fi
# System Events' dock autohide reads back false and leaves the Dock shown, so the Dock restarts.
# After the wallpaper: the relaunched Dock redraws the desktop, which a band check would take for the new picture.
defaults write com.apple.dock autohide -bool true
killall Dock
sleep 2

HOME=$home ./target/release/inkra-app "$root" 2> /dev/null &
pid=$!

# Centred on the main screen; prints the window's frame as "x y w h". Run again after the settle:
# the window can move or take back its default size while it opens.
place() {
  osascript - "$pid" "$width" "$height" "$screen_w" "$screen_h" << 'EOF'
on run argv
  set pid to (item 1 of argv) as integer
  set w to (item 2 of argv) as integer
  set h to (item 3 of argv) as integer
  set screen_w to (item 4 of argv) as integer
  set screen_h to (item 5 of argv) as integer
  tell application "System Events"
    repeat 100 times
      if exists (first process whose unix id is pid) then
        set p to first process whose unix id is pid
        if (count of windows of p) > 0 then exit repeat
      end if
      delay 0.1
    end repeat
    -- An app activated during the settle, a click or a keystroke, would cover the window.
    set visible of every process whose visible is true and unix id is not pid to false
    set win to window 1 of p
    set size of win to {w, h}
    set position of win to {(screen_w - w) div 2, (screen_h - h) div 2}
    set frontmost of p to true
    delay 0.5
    set {x, y} to position of win
    set {w, h} to size of win
  end tell
  return (x as text) & " " & (y as text) & " " & (w as text) & " " & (h as text)
end run
EOF
}
place > /dev/null
sleep "$settle"
read -r x y w h < <(place)
want="$(((${screen_w%.*} - width) / 2)) $(((${screen_h%.*} - height) / 2)) $width $height"
[[ "$x $y $w $h" == "$want" ]] || { echo "the window's frame is $x $y $w $h, not $want; run again" >&2; exit 1; }

mkdir -p "$(dirname "$out")"
screencapture -x -R "$((x - padding)),$((y - padding)),$((w + 2 * padding)),$((h + 2 * padding))" "$home/shot.png"
case $out in
  *.webp) cwebp -quiet -lossless -z 9 -metadata icc "$home/shot.png" -o "$out" ;;
  *) mv "$home/shot.png" "$out" ;;
esac
echo "$out: $(sips -g pixelWidth -g pixelHeight "$out" | awk '/pixel/ {printf "%s ", $2}')px, window ${w}x${h} at ${x},${y}"
