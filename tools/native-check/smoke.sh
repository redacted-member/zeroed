#!/bin/zsh
# Native smoke test of the running dev app (process "carvestudio").
# Usage: smoke.sh  -> prints PASS/FAIL lines and writes smoke_sheet.png (menu sweep) for a visual glance.
S=${0:A:h}; fail=0; mkdir -p $S/files; touch $S/tauri.log
ok() { echo "PASS $1"; }; bad() { echo "FAIL $1"; fail=1; }
n=$(osascript -e 'tell application "System Events" to tell process "carvestudio" to count windows' 2>/dev/null)
[[ $n == 1 ]] && ok "one window" || { bad "window count $n (screen locked?)"; exit 1; }
osascript -e 'tell application "System Events" to tell process "carvestudio" to set position of window 1 to {900, 150}'
before=$(grep -c "could not emit" $S/tauri.log)
osascript $S/menus.applescript > $S/smoke_menus.txt 2>&1 && ok "menu tree readable" || bad "menu tree"
dis=$(grep -E "\[false\]" $S/smoke_menus.txt | grep -vE "AutoFill|Contact|Passwords|Credit Card|Clear Menu|missing value|Swap Sides|Run Last|Save Selected Toolpath as Template|Bake Components|Export as 3D Clipart|Sculpt Visible Model|Smooth Selected Components|Create Vector Boundary Around Selected Components|Emboss Component|Distort Selected Component|Selected Component Properties|Reset Inside|Split Selected Component" )
[[ -z $dis ]] && ok "no unexpected disabled items" || bad "disabled items: $(echo $dis | tr '\n' ';')"
# every enabled leaf item of the app menus except file dialogs and quit
items=()
menu=""
while IFS= read -r line; do
  if [[ $line == "== "* ]]; then menu=${line#== }; continue; fi
  [[ $menu == CarveStudio ]] && continue
  [[ $line == "     > "* ]] && continue
  [[ $line == *"[true]"* ]] || continue
  name=${${line#  }%% \[true\]*}
  case $name in
    Open...|"Save As..."|"Import "*|"Export..."|"Quit "*|Cut|Copy|Paste|AutoFill|"Start Dictation…"|"Emoji & Symbols"|"Enter Full Screen"|"Open Recent"|"Standard 3D Views"|Delete|"Save Toolpaths..."|Save|"Clear Model") continue ;;
  esac
  items+=("$menu|$name")
done < $S/smoke_menus.txt
$S/sweep.sh smoke "${items[@]}" > $S/smoke_sweep.txt 2>&1
for k in $(seq 1 ${#items}); do osascript -e 'tell application "System Events" to tell process "carvestudio" to key code 53' >/dev/null 2>&1; done
after=$(grep -c "could not emit" $S/tauri.log)
[[ $after == $before ]] && ok "menu events delivered (${#items} items swept)" || bad "$((after-before)) menu events failed to emit"
grep -q "sheets" /dev/null
# save / new / open round trip
f=$S/files/smoke.carve; rm -f $f
$S/nat.sh menu File "Save As..." >/dev/null; sleep 1.5
$S/nat.sh key g "command down, shift down" >/dev/null; sleep 0.8
osascript -e "tell application \"System Events\" to keystroke \"$S/files/\"" -e 'delay 0.5' -e 'tell application "System Events" to key code 36' -e 'delay 0.8' -e 'tell application "System Events" to keystroke "a" using command down' -e 'tell application "System Events" to keystroke "smoke"' -e 'delay 0.3' -e 'tell application "System Events" to key code 36'
sleep 1.5; [[ -s $f ]] && ok "save as wrote $(stat -f %z $f) bytes" || bad "save as"
$S/nat.sh menu File New >/dev/null; sleep 1.5
$S/nat.sh menu File "Open..." >/dev/null; sleep 1.5
$S/nat.sh key g "command down, shift down" >/dev/null; sleep 0.8
osascript -e "tell application \"System Events\" to keystroke \"$f\"" -e 'delay 0.6' -e 'tell application "System Events" to key code 36' -e 'delay 0.8' -e 'tell application "System Events" to key code 36'
sleep 2
t=$($S/nat.sh windows | cut -d, -f1)
[[ $t == "smoke.carve - CarveStudio" ]] && ok "reopened, title '$t'" || bad "title after open '$t'"
grep -iE "panicked|error\[" $S/tauri.log | tail -3 | sed 's/^/LOG /'
echo "sheet: $S/smoke_sheet.png"; exit $fail
