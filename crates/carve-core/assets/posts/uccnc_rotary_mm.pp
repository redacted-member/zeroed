' Zeroed post processor for UCCNC with a rotary A axis, millimeters.
' For a machine with a rotary A axis and an M6 macro that probes the tool length.
' A blank along X wraps flat Y into A; a blank along Y wraps flat X into A. A
' is in degrees, one time round the blank is 360, and the wrapped linear axis
' stays on the axis line at zero. Z is written from the job's Z zero, so set
' the machine's Z zero on the rotary axis or on the blank's top to match Job
' Setup. Set the zero of the wrapped linear axis on the rotary axis.
' Wrapped cuts feed in inverse time (G93): each carries one over the minutes
' it takes, so the surface moves at the toolpath's feed on any diameter. Tool
' changes run in G94, because a change routine's feeds, its probing feed too,
' would otherwise be read as one over minutes.
' If a test comes out mirrored the axis is wired the other way: set Direction to
' Reversed in Machine Configuration, or write VAR A_POSITION with a multiplier, [A|A|A|1.3|-1].
' A flat job through this post stays flat with A held at zero.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' Zeroed       2026-11-04  First version.

POST_NAME = "UCCNC rotary A axis, M6 probes tool length (mm) (*.tap)"
FILE_EXTENSION = "tap"
UNITS = MM
RAPID_RATE = 3000
SPINDLE_SPEED_RANGE = 0 24000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"
LINE_NUMBER_START = 10
LINE_NUMBER_INCREMENT = 10
DIRECT_OUTPUT = "UCCNC"
ROTARY_WRAP_Y = A
ROTARY_WRAP_X = A
MAXIMUM_ROTATION = 90
INVERSE_TIME_MODE = YES

VAR LINE_NUMBER = [N|AF|N|1.0]
VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR Z_POSITION = [Z|A|Z|1.3]
VAR A_POSITION = [A|A|A|1.3]
VAR SAFE_HEIGHT = [SAFEZ|AF|Z|1.3]
VAR CLEAR_HEIGHT = [ZCLEAR|AF|Z|1.3]
VAR HOME_Z = [ZH|AF|Z|1.3]
VAR FEED_RATE = [F|AF|F|1.3]
VAR SPINDLE_SPEED = [S|AF|S|1.0]
VAR TOOL_NUMBER = [T|AF||1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.3]
VAR ARC_CENTRE_Y = [J|IF|J|1.3]
VAR DWELL_TIME = [DWELL|AF|P|1.2]

begin HEADER
+ "%"
+ "([TP_FILENAME])"
+ "(Written by Zeroed)"
+ "([TOOLS_USED])"
+ "[N]G90[FEED_MODE]"
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
+ "[N]G94"
+ "[N]T[T]M6"
+ "[N]G43H[T][IF_LENGTH_BY_PROGRAM]"
+ "[N][FEED_MODE]"
+ "[N]G0[ZCLEAR]"
end

begin SPINDLE_ON
+ "[N][S]M3"
end

begin NEW_SEGMENT
+ "([TOOLPATH_NAME])"
end

begin RAPID_MOVE
+ "[N]G0[X][Y][Z][A]"
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
+ "[N]G1[X][Y][Z][A][F]"
end

begin FEED_MOVE
+ "[N]G1[X][Y][Z][A]"
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
+ "[N]G94"
+ "[N]G49[IF_LENGTH_BY_PROGRAM]"
+ "[N]M30"
end

begin TRAILER
+ "%"
end
