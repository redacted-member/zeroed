' Zeroed post processor for Heidenhain conversational (plain language)
' controls, millimeters. Every line is numbered from 0, coordinates carry
' their sign, rapid moves use FMAX, and arcs are written as a circle center
' (CC) followed by a circular move (C) with its direction (DR- clockwise,
' DR+ counter clockwise).
'
' History
' Who          When        What
' ===========  ==========  ==========================================
' Zeroed       2026-09-25  First version.
' Zeroed       2026-09-27  The footer stays at the home height instead of
'                          going back down to safe Z.
' Zeroed       2026-09-27  The spindle stops (M5) before every tool change.
' Zeroed       2026-09-27  After every tool change (and so at the start) Z lifts
'                          straight up to the safe Z or the home Z, whichever is
'                          higher, before the spindle starts or anything moves
'                          across; the first rapid crosses at that height and
'                          only then comes down.
'                          The footer lifts to that height too, so the move
'                          home never crosses below the safe Z.
' Zeroed       2026-09-28  The spindle speed and start are their own
'                          SPINDLE_ON block (TOOL CALL S, a call with only a
'                          speed, which changes nothing else, then M3),
'                          written after the tool change and again wherever
'                          a merged toolpath changes speed.

POST_NAME = "Heidenhain conversational (mm) (*.h)"
FILE_EXTENSION = "h"
UNITS = MM
RAPID_RATE = 3000
SPINDLE_SPEED_RANGE = 0 24000
LINE_ENDING = "[13][10]"
LINE_NUMBER = 0
LINE_NUMBER_INCREMENT = 1

VAR LINE_NUMBER = [N|AF||1.0]
VAR X_POSITION = [X|AFP| X|1.3]
VAR Y_POSITION = [Y|AFP| Y|1.3]
VAR Z_POSITION = [Z|AFP| Z|1.3]
VAR SAFE_HEIGHT = [SAFEZ|AFP| Z|1.3]
VAR CLEAR_HEIGHT = [ZCLEAR|AFP| Z|1.3]
VAR HOME_Z = [ZH|AFP| Z|1.3]
VAR FEED_RATE = [F|AF| F|1.0]
VAR SPINDLE_SPEED = [S|AF| S|1.0]
VAR TOOL_NUMBER = [T|AF||1.0]
VAR ARC_CENTRE_X = [IA|AFP| X|1.3]
VAR ARC_CENTRE_Y = [JA|AFP| Y|1.3]
VAR DWELL_TIME = [DWELL|AF||1.1]

begin HEADER
+ "[N] BEGIN PGM ZEROED MM"
+ "[N] ; [TP_FILENAME], written by Zeroed"
end

begin TOOLCHANGE
+ "[N] ; [TOOL_NAME]"
+ "[N] M5"
+ "[N] TOOL CALL [T] Z"
+ "[N] L[ZCLEAR] R0 FMAX"
end

begin SPINDLE_ON
+ "[N] TOOL CALL[S]"
+ "[N] M3"
end

begin NEW_SEGMENT
+ "[N] ; [TOOLPATH_NAME]"
end

begin RAPID_MOVE
+ "[N] L[X][Y][Z] R0 FMAX"
end

begin RETRACT_MOVE
+ "[N] L[Z] R0 FMAX"
end

begin PLUNGE_MOVE
+ "[N] L[Z] R0[F]"
end

begin FEED_MOVE
+ "[N] L[X][Y][Z] R0[F]"
end

begin CW_ARC_MOVE
+ "[N] CC[IA][JA]"
+ "[N] C[X][Y] DR- R0[F]"
end

begin CCW_ARC_MOVE
+ "[N] CC[IA][JA]"
+ "[N] C[X][Y] DR+ R0[F]"
end

begin CW_HELICAL_ARC_MOVE
+ "[N] CC[IA][JA]"
+ "[N] C[X][Y][Z] DR- R0[F]"
end

begin CCW_HELICAL_ARC_MOVE
+ "[N] CC[IA][JA]"
+ "[N] C[X][Y][Z] DR+ R0[F]"
end

begin DWELL_MOVE
+ "[N] CYCL DEF 9.0 DWELL TIME"
+ "[N] CYCL DEF 9.1 DWELL [DWELL]"
end

begin FOOTER
+ "[N] L[ZCLEAR] R0 FMAX M5"
+ "[N] M30"
+ "[N] END PGM ZEROED MM"
end
