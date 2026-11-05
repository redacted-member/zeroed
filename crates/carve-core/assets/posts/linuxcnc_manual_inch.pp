' CarveStudio post processor for LinuxCNC, inches.
' For a hand tool change with a touch off: at each change the spindle
' lifts clear, a message names the new tool, T M6 and G43 H apply the tool
' table's length and M0 waits for the operator before Z lifts again.
' Uses G64 path blending with a 0.0005 inch tolerance and ends the program with
' M2 as LinuxCNC prefers.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' CarveStudio  2026-09-25  First version.
' CarveStudio  2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z. The tool length offset
'                          is G43 H and the tool number (was G43 HT and it).
' CarveStudio  2026-09-27  The spindle stops (M5) before every tool change.
' CarveStudio  2026-09-27  After every tool change (and so at the start) Z lifts
'                          straight up to the safe Z or the home Z, whichever is
'                          higher, before the spindle starts or anything moves
'                          across; the first rapid crosses at that height and
'                          only then comes down.
'                          The footer lifts to that height too, so the move
'                          home never crosses below the safe Z.
' CarveStudio  2026-09-27  Tool changes are for a hand change: Z lifts clear
'                          before the change, a message names the new tool
'                          and M0 waits for the operator; after it Z lifts
'                          again before anything else moves.
' CarveStudio  2026-09-28  Split from the LinuxCNC post, which now leaves the
'                          tool length to a probing M6: this one keeps the
'                          hand change with a touch off at the M0 pause.
' CarveStudio  2026-09-28  The spindle speed and start are their own SPINDLE_ON
'                          block, written after the tool change as before and
'                          again wherever a merged toolpath changes speed.
' CarveStudio  2026-09-28  The header sets every modal state the program
'                          relies on, since LinuxCNC keeps them from the last
'                          program: G90, G94, units, G17, G40, G49 and G64
'                          (the G43 H after each change sets the length).

POST_NAME = "LinuxCNC manual touch off at M0 (inch) (*.ngc)"
FILE_EXTENSION = "ngc"
UNITS = INCHES
RAPID_RATE = 120
SPINDLE_SPEED_RANGE = 0 24000
PRINT_TOOLLIST = YES
LINE_ENDING = "[10]"

VAR X_POSITION = [X|A|X|1.4]
VAR Y_POSITION = [Y|A|Y|1.4]
VAR Z_POSITION = [Z|A|Z|1.4]
VAR SAFE_HEIGHT = [SAFEZ|AF|Z|1.4]
VAR CLEAR_HEIGHT = [ZCLEAR|AF|Z|1.4]
VAR HOME_Z = [ZH|AF|Z|1.4]
VAR HOME_X = [XH|AF|X|1.4]
VAR HOME_Y = [YH|AF|Y|1.4]
VAR FEED_RATE = [F|AF|F|1.1]
VAR SPINDLE_SPEED = [S|AF|S|1.0]
VAR TOOL_NUMBER = [T|AF||1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.4]
VAR ARC_CENTRE_Y = [J|IF|J|1.4]
VAR DWELL_TIME = [DWELL|AF|P|1.2]

begin HEADER
+ "([TP_FILENAME])"
+ "(Written by CarveStudio)"
+ "([TOOLS_USED])"
+ "G90"
+ "G94"
+ "G20"
+ "G17"
+ "G40"
+ "G49"
+ "G64P0.0005"
end

begin TOOLCHANGE
+ "M5"
+ "G0[ZCLEAR]"
+ "(MSG,Fit tool [T]: [TOOL_NAME], then resume)"
+ "T[T]M6"
+ "G43H[T]"
+ "M0"
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
+ "G0[XH][YH]"
+ "M2"
end
