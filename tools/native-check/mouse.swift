import CoreGraphics
import Foundation
// usage: mouse click x y | mouse drag x1 y1 x2 y2 | mouse dclick x y  (global screen points, top-left origin)
// mouse key 53 [cmd,shift,ctrl,alt]: a real key press (System Events keys do not always reach the web view)
let a = CommandLine.arguments
func post(_ t: CGEventType, _ p: CGPoint, _ clicks: Int64 = 1) {
  let e = CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: .left)!
  e.setIntegerValueField(.mouseEventClickState, value: clicks)
  e.post(tap: .cghidEventTap)
}
let n = a.dropFirst(2).compactMap { Double($0) }
switch a[1] {
case "click":
  let p = CGPoint(x: n[0], y: n[1])
  post(.mouseMoved, p); usleep(50000); post(.leftMouseDown, p); usleep(50000); post(.leftMouseUp, p)
case "dclick":
  let p = CGPoint(x: n[0], y: n[1])
  post(.leftMouseDown, p, 1); post(.leftMouseUp, p, 1); usleep(80000); post(.leftMouseDown, p, 2); post(.leftMouseUp, p, 2)
case "drag":
  let p = CGPoint(x: n[0], y: n[1]), q = CGPoint(x: n[2], y: n[3])
  post(.mouseMoved, p); usleep(50000); post(.leftMouseDown, p)
  for i in 1...20 { let t = Double(i)/20; post(.leftMouseDragged, CGPoint(x: p.x+(q.x-p.x)*t, y: p.y+(q.y-p.y)*t)); usleep(20000) }
  post(.leftMouseUp, q)
case "scroll":
  let p = CGPoint(x: n[0], y: n[1])
  post(.mouseMoved, p); usleep(50000)
  let e = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: Int32(n[2]), wheel2: 0, wheel3: 0)!
  e.location = p; e.post(tap: .cghidEventTap)
case "key":
  var flags = CGEventFlags()
  if a.count > 3 {
    for f in a[3].split(separator: ",") {
      switch f {
      case "cmd": flags.insert(.maskCommand)
      case "shift": flags.insert(.maskShift)
      case "ctrl": flags.insert(.maskControl)
      case "alt": flags.insert(.maskAlternate)
      default: break
      }
    }
  }
  for down in [true, false] {
    let e = CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(n[0]), keyDown: down)!
    e.flags = flags
    e.post(tap: .cghidEventTap)
    usleep(30000)
  }
default: break
}
