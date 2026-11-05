' Zeroed post processor for Centroid controls, millimeters. Centroid
' reads plain G code with G43 tool length offsets and ends with M30; arcs
' that move in Z are written as helical moves.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' Zeroed       2026-09-25  First version.
' Zeroed       2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z.
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

POST_NAME = "Centroid (mm) (*.nc)"
FILE_EXTENSION = "nc"
UNITS = MM
RAPID_RATE = 3000
SPINDLE_SPEED_RANGE = 0 24000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"

VAR X_POSITION = [X|A| X|1.4]
VAR Y_POSITION = [Y|A| Y|1.4]
VAR Z_POSITION = [Z|A| Z|1.4]
VAR SAFE_HEIGHT = [SAFEZ|AF| Z|1.4]
VAR CLEAR_HEIGHT = [ZCLEAR|AF| Z|1.4]
VAR HOME_Z = [ZH|AF| Z|1.4]
VAR FEED_RATE = [F|AF| F|1.1]
VAR SPINDLE_SPEED = [S|AF| S|1.0]
VAR TOOL_NUMBER = [T|AF||1.0]
VAR ARC_CENTRE_X = [I|IF| I|1.4]
VAR ARC_CENTRE_Y = [J|IF| J|1.4]
VAR DWELL_TIME = [DWELL|AF| P|1.2]

begin HEADER
+ "; [TP_FILENAME]"
+ "; Written by Zeroed"
+ "; [TOOLS_USED]"
+ "G90 G17 G40 G49 G80"
+ "G21"
end

begin TOOLCHANGE
+ "; [TOOL_NAME]"
+ "M5"
+ "T[T] H[T] M6"
+ "G43 H[T]"
+ "G0[ZCLEAR]"
end

begin SPINDLE_ON
+ "M3[S]"
end

begin NEW_SEGMENT
+ "; [TOOLPATH_NAME]"
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

begin CW_ARC_MOVE
+ "G2[X][Y][I][J]"
end

begin CCW_ARC_MOVE
+ "G3[X][Y][I][J]"
end

begin CW_HELICAL_ARC_MOVE
+ "G2[X][Y][Z][I][J]"
end

begin CCW_HELICAL_ARC_MOVE
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
