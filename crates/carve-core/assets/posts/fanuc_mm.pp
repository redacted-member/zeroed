' CarveStudio post processor for Fanuc style controls, millimeters. The
' program starts with a percent sign and a program number, each tool change
' calls tool length compensation (G43 H) with the tool's own number, and
' arcs that move in Z are written as helical G2 and G3 moves.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' CarveStudio  2026-09-25  First version.
' CarveStudio  2026-09-27  The tool length call moves in G0: nothing before it
'                          sets a motion mode, so the control would otherwise
'                          use its power on mode (G1 alarms with no feed).
' CarveStudio  2026-09-27  The tool length call lifts to the safe Z or the home
'                          Z, whichever is higher, and the first rapid crosses
'                          at that height before it comes down.
' CarveStudio  2026-09-28  The spindle speed and start are their own SPINDLE_ON
'                          block, written once the tool change has lifted to the
'                          clearance height (as the other posts do) and again
'                          wherever a merged toolpath changes speed.

POST_NAME = "Fanuc (mm) (*.nc)"
FILE_EXTENSION = "nc"
UNITS = MM
RAPID_RATE = 3000
SPINDLE_SPEED_RANGE = 0 24000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"
LINE_NUMBER = 10
LINE_NUMBER_INCREMENT = 10
LINE_NUMBER_MAXIMUM = 99990

VAR LINE_NUMBER = [N|AF|N|1.0]
VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR Z_POSITION = [Z|A|Z|1.3]
VAR SAFE_HEIGHT = [SAFEZ|AF|Z|1.3]
VAR CLEAR_HEIGHT = [ZCLEAR|AF|Z|1.3]
VAR FEED_RATE = [F|AF|F|1.1]
VAR SPINDLE_SPEED = [S|AF|S|1.0]
VAR TOOL_NUMBER = [T|AF||1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.3]
VAR ARC_CENTRE_Y = [J|IF|J|1.3]
VAR DWELL_TIME = [DWELL|AF|P|1.0|1000]

begin HEADER
+ "%"
+ "O0001"
+ "([TP_FILENAME])"
+ "(Written by CarveStudio)"
+ "([TOOLS_USED])"
+ "[N]G90G94G17G40G49G80"
+ "[N]G21"
end

begin TOOLCHANGE
+ "[N]M5"
+ "[N]G91G28Z0"
+ "[N]G90"
+ "([TOOL_NAME])"
+ "[N]T[T]M6"
+ "[N]G0G43H[T][ZCLEAR]"
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

begin FEED_RATE_CHANGE
+ "[N][F]"
end

begin CW_ARC_MOVE
+ "[N]G2[X][Y][I][J]"
end

begin CCW_ARC_MOVE
+ "[N]G3[X][Y][I][J]"
end

begin CW_HELICAL_ARC_MOVE
+ "[N]G2[X][Y][Z][I][J]"
end

begin CCW_HELICAL_ARC_MOVE
+ "[N]G3[X][Y][Z][I][J]"
end

begin DWELL_MOVE
+ "[N]G4[DWELL]"
end

begin FOOTER
+ "[N]M5"
+ "[N]G91G28Z0"
+ "[N]G90"
+ "[N]M30"
end

begin TRAILER
+ "%"
end
