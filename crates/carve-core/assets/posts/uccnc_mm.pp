' CarveStudio post processor for UCCNC controllers, millimeters.
' For a hand tool change where the M6 macro probes the tool length on a
' fixed sensor: at each change the spindle stops and lifts clear, a comment
' names the new tool, T M6 runs the macro (it waits for the change, probes
' and sets Z zero for the new tool itself), then Z lifts again before the
' spindle starts. No G43 H follows: the macro's Z zero already holds the
' length, and a tool table offset would be added on top of it.
' For a touch off at a pause instead, use "UCCNC manual touch off at M0".
' Percent signs wrap the program, lines are numbered and the file is written
' with carriage return and line feed endings.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' CarveStudio  2026-09-25  First version.
' CarveStudio  2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z.
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
' CarveStudio  2026-09-28  Tool changes are for an M6 that probes the tool
'                          length: no M0 after the change (the
'                          hand change with M0 is its own post now).
' CarveStudio  2026-09-28  The spindle speed and start are their own SPINDLE_ON
'                          block, written after the tool change as before and
'                          again wherever a merged toolpath changes speed.
' CarveStudio  2026-09-28  The header sets every modal state the program
'                          relies on, since UCCNC keeps them from the last
'                          program: G90, G94, G21, G17, G40 and G64 (with
'                          the screen's blending settings: UCCNC's P is not
'                          a tolerance). It writes no G49: the M6 macro sets
'                          the length, and one that skips a change to the
'                          tool already fitted would leave it cancelled.
' CarveStudio  2026-10-03  With "M6 macro applies tool length" off in Save
'                          Toolpaths (a macro that probes into the tool
'                          table only), G43 H and the tool number follow
'                          each M6, and G49 is in the header (as in the
'                          manual posts) and ends the program.

POST_NAME = "UCCNC M6 probes tool length (mm) (*.tap)"
FILE_EXTENSION = "tap"
UNITS = MM
RAPID_RATE = 3000
SPINDLE_SPEED_RANGE = 0 24000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"
LINE_NUMBER_START = 10
LINE_NUMBER_INCREMENT = 10
DIRECT_OUTPUT = "UCCNC"

VAR LINE_NUMBER = [N|AF|N|1.0]
VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR Z_POSITION = [Z|A|Z|1.3]
VAR SAFE_HEIGHT = [SAFEZ|AF|Z|1.3]
VAR CLEAR_HEIGHT = [ZCLEAR|AF|Z|1.3]
VAR HOME_Z = [ZH|AF|Z|1.3]
VAR FEED_RATE = [F|AF|F|1.1]
VAR SPINDLE_SPEED = [S|AF|S|1.0]
VAR TOOL_NUMBER = [T|AF||1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.3]
VAR ARC_CENTRE_Y = [J|IF|J|1.3]
VAR DWELL_TIME = [DWELL|AF|P|1.2]

begin HEADER
+ "%"
+ "([TP_FILENAME])"
+ "(Written by CarveStudio)"
+ "([TOOLS_USED])"
+ "[N]G90G94"
+ "[N]G21"
+ "[N]G17"
+ "[N]G40"
+ "[N]G49[IF_LENGTH_BY_PROGRAM]"
+ "[N]G64"
end

begin TOOLCHANGE
+ "[N]M5"
+ "[N]G0[ZCLEAR]"
+ "(Change to tool T[T]: [TOOL_NAME], M6 probes its length)"
+ "[N]T[T]M6"
+ "[N]G43H[T][IF_LENGTH_BY_PROGRAM]"
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
+ "[N]G49[IF_LENGTH_BY_PROGRAM]"
+ "[N]M30"
end

begin TRAILER
+ "%"
end
