' Zeroed post processor for a GRBL laser, millimeters.
' The beam is driven with M4 dynamic power and turned off for every travel
' move. Power is a percentage in the tool database, so it is scaled to the
' 0 to 1000 range GRBL expects with $30 left at its default.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' Zeroed       2026-09-20  First version.

POST_NAME = "GRBL laser (mm) (*.gcode)"
FILE_EXTENSION = "gcode"
UNITS = MM
RAPID_RATE = 6000
PRINT_TOOLLIST = NO
LINE_ENDING = "[13][10]"

VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR HOME_X = [XH|AF|X|1.3]
VAR HOME_Y = [YH|AF|Y|1.3]
VAR FEED_RATE = [F|AF|F|1.1]
VAR LASER_POWER = [POWER|AF|S|1.0|10]

begin HEADER
+ "([TP_FILENAME])"
+ "(Written by Zeroed)"
+ "G90"
+ "G21"
+ "G17"
+ "M5"
end

begin NEW_SEGMENT
+ "([TOOLPATH_NAME])"
end

begin RAPID_MOVE
+ "G0[X][Y]"
end

begin FIRST_FEED_MOVE
+ "G1[X][Y][F]"
end

begin FEED_MOVE
+ "G1[X][Y]"
end

begin JET_TOOL_POWER
+ "[POWER]"
end

begin JET_TOOL_ON
+ "M4"
end

begin JET_TOOL_OFF
+ "M5"
end

begin FOOTER
+ "M5"
+ "G0[XH][YH]"
+ "M30"
end
