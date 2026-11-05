#!/bin/zsh
# usage: nat.sh menu "File" "New"   |  nat.sh shot name  | nat.sh key "s" "command down" | nat.sh front | nat.sh windows
S=${0:h}
case $1 in
  front) osascript -e 'tell application "System Events" to set frontmost of process "carvestudio" to true' ;;
  menu) osascript -e "tell application \"System Events\" to tell process \"carvestudio\"
      set frontmost to true
      delay 0.3
      click menu item \"$3\" of menu 1 of menu bar item \"$2\" of menu bar 1
    end tell" ;;
  submenu) osascript -e "tell application \"System Events\" to tell process \"carvestudio\"
      set frontmost to true
      delay 0.3
      click menu item \"$4\" of menu 1 of menu item \"$3\" of menu 1 of menu bar item \"$2\" of menu bar 1
    end tell" ;;
  key) osascript -e "tell application \"System Events\" to tell process \"carvestudio\"
      set frontmost to true
      delay 0.2
      keystroke \"$2\" using {$3}
    end tell" ;;
  keycode) osascript -e "tell application \"System Events\" to tell process \"carvestudio\"
      set frontmost to true
      delay 0.2
      key code $2
    end tell" ;;
  windows) osascript -e 'tell application "System Events" to tell process "carvestudio" to get {name, position, size} of every window' ;;
  shot)
    b=$(osascript -e 'tell application "System Events" to tell process "carvestudio"
      set p to position of window 1
      set s to size of window 1
      return (item 1 of p as text) & "," & (item 2 of p as text) & "," & (item 1 of s as text) & "," & (item 2 of s as text)
    end tell')
    screencapture -x -R$b $S/$2.png && sips -Z 1400 $S/$2.png >/dev/null && echo $S/$2.png ;;
esac
if [[ $1 == click ]]; then
  # image coords from a 1400-wide shot of the 1440-wide window at 2380,300
  pos=(${(s:,:)$(osascript -e "tell application \"System Events\" to tell process \"carvestudio\" to get position of window 1" | tr -d " ")})
  x=$(( pos[1] + $2 * 1440 / 1400 )); y=$(( pos[2] + $3 * 1440 / 1400 ))
  osascript -e "tell application \"System Events\" to tell process \"carvestudio\"
    set frontmost to true
    delay 0.2
    click at {$x, $y}
  end tell"
fi
