' Zeroed post processor for a plain G code controller, millimeters.
' Numbered lines, an automatic tool change and arcs with incremental centers.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' Zeroed       2026-09-20  First version.
' Zeroed       2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z.
' Zeroed       2026-09-27  The spindle stops (M5) before every tool change.
' Zeroed       2026-09-27  After every tool change (and so at the start) Z lifts
'                          straight up to the safe Z or the home Z, whichever is
'                          higher, before the spindle starts or anything moves
'                          across; the first rapid crosses at that height and
'                          only then comes down.
'                          The footer lifts to that height too, so the move
'                          home never crosses below the safe Z.
' Zeroed       2026-09-28  The spindle speed and start are their own SPINDLE_ON
'                          block, written after the tool change as before and
'                          again wherever a merged toolpath changes speed.

POST_NAME = "Generic G code (mm) (*.gcode)"
FILE_EXTENSION = "gcode"
UNITS = MM
RAPID_RATE = 3000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"
LINE_NUMBER_START = 10
LINE_NUMBER_INCREMENT = 10

VAR LINE_NUMBER = [N|AF|N|1.0]
VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR Z_POSITION = [Z|A|Z|1.3]
VAR SAFE_HEIGHT = [SAFEZ|AF|Z|1.3]
VAR CLEAR_HEIGHT = [ZCLEAR|AF|Z|1.3]
VAR HOME_Z = [ZH|AF|Z|1.3]
VAR HOME_X = [XH|AF|X|1.3]
VAR HOME_Y = [YH|AF|Y|1.3]
VAR FEED_RATE = [F|AF|F|1.1]
VAR SPINDLE_SPEED = [S|AF|S|1.0]
VAR TOOL_NUMBER = [T|AF|T|1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.3]
VAR ARC_CENTRE_Y = [J|IF|J|1.3]
VAR DWELL_TIME = [DWELL|AF|P|1.2]

begin HEADER
+ "([TP_FILENAME])"
+ "(Written by Zeroed)"
+ "([TOOLS_USED])"
+ "[N]G90"
+ "[N]G21"
+ "[N]G17"
+ "[N]G40"
end

begin TOOLCHANGE
+ "[N]M5"
+ "[N][T]M6"
+ "[N]G0[ZCLEAR]"
end

begin SPINDLE_ON
+ "[N][S]M3"
end

begin NEW_SEGMENT
+ "([TOOLPATH_NAME])"
end

begin RAPID_MOVE
+ "[N]G0[X][Y][Z]"
end

begin RETRACT_MOVE
+ "[N]G0[Z]"
end

begin FIRST_PLUNGE_MOVE
+ "[N]G1[Z][F]"
end

begin PLUNGE_MOVE
+ "[N]G1[Z]"
end

begin FIRST_FEED_MOVE
+ "[N]G1[X][Y][Z][F]"
end

begin FEED_MOVE
+ "[N]G1[X][Y][Z]"
end

begin FIRST_CW_ARC_MOVE
+ "[N]G2[X][Y][Z][I][J][F]"
end

begin CW_ARC_MOVE
+ "[N]G2[X][Y][Z][I][J]"
end

begin FIRST_CCW_ARC_MOVE
+ "[N]G3[X][Y][Z][I][J][F]"
end

begin CCW_ARC_MOVE
+ "[N]G3[X][Y][Z][I][J]"
end

begin DWELL_MOVE
+ "[N]G4[DWELL]"
end

begin FOOTER
+ "[N]M5"
+ "[N]G0[ZCLEAR]"
+ "[N]G0[XH][YH]"
+ "[N]M30"
end
