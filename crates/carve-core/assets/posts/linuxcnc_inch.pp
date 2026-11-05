' Zeroed post processor for LinuxCNC, inches.
' For a hand tool change where M6 is remapped to probe the tool length on
' a tool setter: at each change the spindle stops and lifts clear, a message
' names the new tool, T M6 runs the remap (it waits for the change, probes
' and applies the measured length itself, with G43.1 or with G10 L1 and
' G43), then Z lifts again before the spindle starts. No G43 H follows the
' change: it would replace the probed length with the tool table's entry.
' For a touch off at a pause instead, use "LinuxCNC manual touch off at M0".
' Uses G64 path blending with a 0.0005 inch tolerance and ends the program with
' M2 as LinuxCNC prefers.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' Zeroed       2026-09-25  First version.
' Zeroed       2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z. The tool length offset
'                          is G43 H and the tool number (was G43 HT and it).
' Zeroed       2026-09-27  The spindle stops (M5) before every tool change.
' Zeroed       2026-09-27  After every tool change (and so at the start) Z lifts
'                          straight up to the safe Z or the home Z, whichever is
'                          higher, before the spindle starts or anything moves
'                          across; the first rapid crosses at that height and
'                          only then comes down.
'                          The footer lifts to that height too, so the move
'                          home never crosses below the safe Z.
' Zeroed       2026-09-27  Tool changes are for a hand change: Z lifts clear
'                          before the change, a message names the new tool
'                          and M0 waits for the operator; after it Z lifts
'                          again before anything else moves.
' Zeroed       2026-09-28  Tool changes are for an M6 that probes the tool
'                          length: no G43 H and no M0 after the change (the
'                          hand change with M0 is its own post now).
' Zeroed       2026-09-28  The spindle speed and start are their own SPINDLE_ON
'                          block, written after the tool change as before and
'                          again wherever a merged toolpath changes speed.
' Zeroed       2026-09-28  The header sets every modal state the program
'                          relies on, since LinuxCNC keeps them from the last
'                          program: G90, G94, units, G17, G40 and G64. It
'                          writes no G49: the M6 applies the probed length,
'                          and a routine that skips a change to the tool
'                          already fitted would leave it cancelled.
' Zeroed       2026-10-03  With "M6 macro applies tool length" off in Save
'                          Toolpaths (a macro that probes into the tool
'                          table only), G43 H and the tool number follow
'                          each M6, and G49 is in the header (as in the
'                          manual posts) and ends the program.

POST_NAME = "LinuxCNC M6 probes tool length (inch) (*.ngc)"
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
+ "(Written by Zeroed)"
+ "([TOOLS_USED])"
+ "G90"
+ "G94"
+ "G20"
+ "G17"
+ "G40"
+ "G49[IF_LENGTH_BY_PROGRAM]"
+ "G64P0.0005"
end

begin TOOLCHANGE
+ "M5"
+ "G0[ZCLEAR]"
+ "(MSG,Change to tool [T]: [TOOL_NAME], M6 probes its length)"
+ "T[T]M6"
+ "G43H[T][IF_LENGTH_BY_PROGRAM]"
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
+ "G49[IF_LENGTH_BY_PROGRAM]"
+ "M2"
end
