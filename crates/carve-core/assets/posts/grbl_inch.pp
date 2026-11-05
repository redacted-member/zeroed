' Zeroed post processor for GRBL controllers, inches.
' GRBL has no automatic tool changer, so a tool change stops the spindle and
' pauses the program with M0 while the operator fits the next cutter.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' Zeroed       2026-09-20  First version.
' Zeroed       2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z.
' Zeroed       2026-09-27  After the pause for a tool change the tool goes
'                          straight up to the home height before the spindle
'                          starts, so it never starts or moves across with the
'                          new cutter still down at the material from zeroing.
'                          The tool number is written before that lift.
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

POST_NAME = "GRBL (inch) (*.gcode)"
FILE_EXTENSION = "gcode"
UNITS = INCHES
RAPID_RATE = 120
SPINDLE_SPEED_RANGE = 1000 24000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"

VAR X_POSITION = [X|A|X|1.4]
VAR Y_POSITION = [Y|A|Y|1.4]
VAR Z_POSITION = [Z|A|Z|1.4]
VAR SAFE_HEIGHT = [SAFEZ|AF|Z|1.4]
VAR CLEAR_HEIGHT = [ZCLEAR|AF|Z|1.4]
VAR HOME_Z = [ZH|AF|Z|1.4]
VAR FEED_RATE = [F|AF|F|1.1]
VAR SPINDLE_SPEED = [S|AF|S|1.0]
VAR TOOL_NUMBER = [T|AF|T|1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.4]
VAR ARC_CENTRE_Y = [J|IF|J|1.4]
VAR DWELL_TIME = [DWELL|AF|P|1.2]

begin HEADER
+ "(Written by Zeroed)"
+ "([TP_FILENAME])"
+ "([TOOLS_USED])"
+ "G90"
+ "G20"
+ "G17"
end

begin TOOLCHANGE
+ "M5"
+ "(Fit [TOOL_NAME])"
+ "M0"
+ "[T]"
+ "G0[ZCLEAR]"
end

begin SPINDLE_ON
+ "[S]M3"
end

begin NEW_SEGMENT
+ "([TOOLPATH_NAME])"
end

begin RAPID_MOVE
+ "G0[X][Y][Z]"
end

begin RETRACT_MOVE
+ "G0[Z]"
end

begin FIRST_PLUNGE_MOVE
+ "G1[Z][F]"
end

begin PLUNGE_MOVE
+ "G1[Z]"
end

begin FIRST_FEED_MOVE
+ "G1[X][Y][Z][F]"
end

begin FEED_MOVE
+ "G1[X][Y][Z]"
end

begin FIRST_CW_ARC_MOVE
+ "G2[X][Y][Z][I][J][F]"
end

begin CW_ARC_MOVE
+ "G2[X][Y][Z][I][J]"
end

begin FIRST_CCW_ARC_MOVE
+ "G3[X][Y][Z][I][J][F]"
end

begin CCW_ARC_MOVE
+ "G3[X][Y][Z][I][J]"
end

begin DWELL_MOVE
+ "G4[DWELL]"
end

begin FOOTER
+ "M5"
+ "G0[ZCLEAR]"
+ "M30"
end
