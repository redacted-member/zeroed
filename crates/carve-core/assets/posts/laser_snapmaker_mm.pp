' CarveStudio post processor for the Snapmaker 2.0 laser module, millimeters.
' M4 varies the power with the speed for smoother engraving. S is the power from
' 0 to 255, which is 2.55 times the toolpath's percent. No Z is written: set the
' focus on the machine before you run the file.
' Written from the Snapmaker 2.0 G-code reference: firmware based on Marlin,
' version 1.20.2 or later. Comments are written with a semicolon, because the
' firmware does not read parentheses as comments. G2 and G3 are listed as
' unverified, so arcs go out as straight moves. There is no M6: a laser job
' uses one module.
' G0 is a straight move at its own F, so every G0 carries the rapid rate and
' every run of G1 carries its feed. G4 P counts milliseconds, so dwells are G4 S
' in seconds. M30 and M2 are not listed: the file ends with M400, finish moves.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' CarveStudio  2026-11-04  First version.

POST_NAME = "Snapmaker 2.0 laser (mm) (*.nc)"
FILE_EXTENSION = "nc"
UNITS = MM
RAPID_RATE = 6000
PRINT_TOOLLIST = NO
LINE_ENDING = "[10]"
LASER_SUPPORT = YES

VAR X_POSITION = [X|A| X|1.3]
VAR Y_POSITION = [Y|A| Y|1.3]
VAR HOME_X = [XH|AF| X|1.3]
VAR HOME_Y = [YH|AF| Y|1.3]
VAR FEED_RATE = [F|AF| F|1.1]
VAR RAPID_RATE = [FR|AF| F|1.0]
VAR LASER_POWER = [POWER|AF| S|1.0|2.55]

begin HEADER
+ "; [TP_FILENAME]"
+ "; Written by CarveStudio"
+ "G90"
+ "G21"
+ "M5"
end

begin NEW_SEGMENT
+ "; [TOOLPATH_NAME]"
end

begin RAPID_MOVE
+ "G0[X][Y][FR]"
end

begin FIRST_FEED_MOVE
+ "G1[X][Y][F]"
end

begin FEED_MOVE
+ "G1[X][Y]"
end

begin JET_TOOL_POWER
+ "M4[POWER]"
end

begin JET_TOOL_ON
+ "M4"
end

begin JET_TOOL_OFF
+ "M5"
end

begin FOOTER
+ "M5"
+ "G0[XH][YH][FR]"
+ "M400"
end
