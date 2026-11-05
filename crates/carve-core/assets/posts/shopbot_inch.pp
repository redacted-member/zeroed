' CarveStudio post processor for ShopBot controllers, inches.
' ShopBot part files are comma separated commands rather than G code, and
' their move and jog speeds are units per second, so the speed variables
' carry a scale factor that turns inches per minute into per second.
' This post writes no arcs, so arc moves come out as short straight moves.
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' CarveStudio  2026-09-25  First version.
' CarveStudio  2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z.
' CarveStudio  2026-09-27  The file checks the machine is set to
'                          inches (system variable 25) before it moves
'                          and stops with a message if not; the spindle
'                          stops (C7) before every tool change; dwells are
'                          written as PAUSE.
' CarveStudio  2026-09-27  After every tool change (and so at the start) Z lifts
'                          straight up to the safe Z or the home Z, whichever is
'                          higher, before the spindle starts or anything moves
'                          across; the first rapid crosses at that height and
'                          only then comes down.
'                          The footer lifts to that height too, so the move
'                          home never crosses below the safe Z.
' CarveStudio  2026-09-28  The move speeds, spindle speed and start (MS, TR,
'                          C6) are their own SPINDLE_ON block, written after
'                          the tool change and again wherever a merged
'                          toolpath changes speed: TR sets the new speed on
'                          the running spindle, MS the part's own feeds. MS
'                          now comes before TR.

POST_NAME = "ShopBot (inch) (*.sbp)"
FILE_EXTENSION = "sbp"
UNITS = INCHES
RAPID_RATE = 120
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"

VAR X_POSITION = [X|AF||1.4]
VAR Y_POSITION = [Y|AF||1.4]
VAR Z_POSITION = [Z|AF||1.4]
VAR SAFE_HEIGHT = [SAFEZ|AF||1.4]
VAR CLEAR_HEIGHT = [ZCLEAR|AF||1.4]
VAR HOME_Z = [ZH|AF||1.4]
VAR HOME_X = [XH|AF||1.4]
VAR HOME_Y = [YH|AF||1.4]
VAR MOVE_SPEED = [F|AF||1.2|0.016666667]
VAR PLUNGE_SPEED = [FP|AF||1.2|0.016666667]
VAR JOG_SPEED = [FR|AF||1.2|0.016666667]
VAR SPINDLE_SPEED = [S|AF||1.0]
VAR TOOL_NUMBER = [T|AF||1.0]
VAR DWELL_TIME = [DWELL|AF||1.2]

begin HEADER
+ "'[TP_FILENAME]"
+ "'Written by CarveStudio"
+ "'[TOOLS_USED]"
+ "SA"
+ "IF %(25)=1 THEN GOTO UNIT_ERROR"
+ "JS,[FR],[FR]"
end

begin TOOLCHANGE
+ "C7"
+ "'Fit [TOOL_NAME]"
+ "&Tool = [T]"
+ "C9"
+ "JZ,[ZCLEAR]"
end

begin SPINDLE_ON
+ "MS,[F],[FP]"
+ "TR,[S]"
+ "C6"
end

begin NEW_SEGMENT
+ "'[TOOLPATH_NAME]"
end

begin RAPID_MOVE
+ "J3,[X],[Y],[Z]"
end

begin RETRACT_MOVE
+ "JZ,[Z]"
end

begin PLUNGE_MOVE
+ "MZ,[Z]"
end

begin FEED_MOVE
+ "M3,[X],[Y],[Z]"
end

begin DWELL_MOVE
+ "PAUSE [DWELL]"
end

begin FOOTER
+ "C7"
+ "JZ,[ZCLEAR]"
+ "J2,[XH],[YH]"
+ "END"
+ "UNIT_ERROR:"
+ "'This file is in inches. Set the ShopBot to inches and run it again."
+ "PAUSE"
+ "END"
end
