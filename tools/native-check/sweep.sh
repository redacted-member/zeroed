#!/bin/zsh
# sweep.sh tag "Menu|Item" ...   : click each, shot, list windows/sheets, press Escape
S=${0:A:h}; tag=$1; shift; i=0
for spec in "$@"; do
  i=$((i+1)); m=${spec%%|*}; it=${spec#*|}
  if [[ $it == *">"* ]]; then $S/nat.sh submenu "$m" "${it%%>*}" "${it#*>}" >/dev/null 2>&1; else $S/nat.sh menu "$m" "$it" >/dev/null 2>&1; fi
  sleep 1.5
  w=$(osascript -e 'tell application "System Events" to tell process "zeroed" to get {name of every window, count of sheets of window 1}' 2>&1)
  echo "$i $spec -> windows: $w"
  screencapture -x $S/_full.png
  b=$(osascript -e 'tell application "System Events" to tell process "zeroed"
      set p to position of window 1
      set s to size of window 1
      return (item 1 of p as text) & "," & (item 2 of p as text) & "," & (item 1 of s as text) & "," & (item 2 of s as text)
    end tell')
  screencapture -x -R$b $S/${tag}_$i.png
  osascript -e "tell application \"System Events\" to key code 53"; sleep 0.5; osascript -e "tell application \"System Events\" to key code 53"; sleep 0.5
done
python3 - $S $tag $i <<'PY'
import sys; from PIL import Image
S,tag,n=sys.argv[1],sys.argv[2],int(sys.argv[3])
ims=[Image.open(f"{S}/{tag}_{k}.png") for k in range(1,n+1)]
w=560; ims=[im.resize((w,int(im.height*w/im.width))) for im in ims]
h=ims[0].height; cols=3; rows=(n+cols-1)//cols
sheet=Image.new('RGB',(w*cols,h*rows),'white')
for k,im in enumerate(ims): sheet.paste(im,((k%cols)*w,(k//cols)*h))
sheet.save(f"{S}/{tag}_sheet.png"); print(f"{S}/{tag}_sheet.png")
PY
