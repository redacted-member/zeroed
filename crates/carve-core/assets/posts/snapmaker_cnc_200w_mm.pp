' CarveStudio post processor for Snapmaker 2.0 200W CNC, millimeters.
' M3 S sets the spindle speed in rpm, 8000 to 18000.
' Written from the Snapmaker 2.0 G-code reference: firmware based on Marlin,
' version 1.20.2 or later. Comments are written with a semicolon, because the
' firmware does not read parentheses as comments. G2 and G3 are listed as
' unverified, so arcs go out as straight moves. There is no M6: each tool goes
' in its own file, and you zero Z on the machine before you run it.
' G0 is a straight move at its own F, so every G0 carries the rapid rate and
' every run of G1 carries its feed. G4 P counts milliseconds, so dwells are G4 S
' in seconds. M30 and M2 are not listed: the file ends with M400, finish moves.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' CarveStudio  2026-11-04  First version.

POST_NAME = "Snapmaker 2.0 200W CNC (mm) (*.cnc)"
FILE_EXTENSION = "cnc"
UNITS = MM
RAPID_RATE = 3000
SPINDLE_SPEED_RANGE = 8000 18000
PRINT_TOOLLIST = YES
LINE_ENDING = "[10]"

VAR X_POSITION = [X|A| X|1.3]
VAR Y_POSITION = [Y|A| Y|1.3]
VAR Z_POSITION = [Z|A| Z|1.3]
VAR CLEAR_HEIGHT = [ZCLEAR|AF| Z|1.3]
VAR HOME_X = [XH|AF| X|1.3]
VAR HOME_Y = [YH|AF| Y|1.3]
VAR FEED_RATE = [F|AF| F|1.1]
VAR RAPID_RATE = [FR|AF| F|1.0]
VAR SPINDLE_SPEED = [S|AF| S|1.0]
VAR DWELL_TIME = [DWELL|AF| S|1.2]

begin HEADER
+ "; [TP_FILENAME]"
+ "; Written by CarveStudio"
+ "; [TOOLS_USED]"
+ "G90"
+ "G21"
+ "G0[ZCLEAR][FR]"
end

begin SPINDLE_ON
+ "M3[S]"
end

begin NEW_SEGMENT
+ "; [TOOLPATH_NAME]"
end

begin NEW_SEGMENT_POST_TOOLCHANGE
+ "; [TOOLPATH_NAME]"
end

begin RAPID_MOVE
+ "G0[X][Y][Z][FR]"
end

begin RETRACT_MOVE
+ "G0[Z][FR]"
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

begin DWELL_MOVE
+ "G4[DWELL]"
end

begin FOOTER
+ "M5"
+ "G0[ZCLEAR][FR]"
+ "G0[XH][YH][FR]"
+ "M400"
end
