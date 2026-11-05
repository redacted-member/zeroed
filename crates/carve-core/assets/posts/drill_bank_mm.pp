' CarveStudio post processor for a G code controller with a drill bank,
' millimeters. As the generic post, plus the drill bank sections.
'
' Setting it up for a machine
' ===========================
' A Drill Bank toolpath moves the head so that its reference drill (the
' Central Drill of the toolpath's template) sits over a hole, then plunges,
' pecks, dwells and retracts as a Drilling toolpath does. The X, Y and Z
' this post writes are the reference drill's tip.
'
' A drill bank toolpath is written in this order:
'   1. SETUP_DRILL_BANK, once, before the toolpath's first move.
'   2. For each firing: the rapid at Safe Z to the reference drill's X and
'      Y, then DRILL_BANK_PATTERN when the set of drills to lower differs
'      from the set already down (a firing that wants the same drills
'      writes nothing), then the plunge, peck, dwell and retract moves.
'   3. FINAL_DRILL_BANK_ACTION, once, after the toolpath's last move.
'
' Values the drill bank sections can use (declare them with VAR, below):
'   [DB]   the drills down as a bit pattern: drill 1 is worth 1, drill 2
'          is worth 2, drill 3 is worth 4 and so on, added up (drills 1, 2
'          and 5 give 19).
'   [DBX]  the same, only the drills on the horizontal arm (the Central
'          Drill's row of the template).
'   [DBY]  the same, only the drills on the vertical arm (the Central
'          Drill's column).
'   [DL]   the drills down as a list. VAR DRILL_LIST = [DL|P|S|F] writes
'          each number after the text P, the first two parted by F and the
'          rest by S: [DL|D|,| ] gives D1 D2,D5.
'   [CD]   the one drill being lowered or raised (per drill posts only).
' The numbers are the Drill Numbers typed in the template, so number the
' drills there as your machine numbers them. The drill bank values are
' written every time they appear.
'
' A controller that lowers drills one at a time uses the per drill
' sections instead of DRILL_BANK_PATTERN: DRILL_BANK_EXTEND_DRILL is
' written for each drill to lower, with [CD] holding its number, and
' DRILL_BANK_RETRACT_DRILL for each drill to raise. At each firing the
' drills that are down but not wanted are raised first, then the wanted
' ones not yet down are lowered; every drill still down is raised before
' FINAL_DRILL_BANK_ACTION. Define either DRILL_BANK_PATTERN or both per
' drill sections, never all three.
'
' As shipped, the sections write only comments, such as
' (DRILLS 1,2,5 PATTERN 19), so the file runs on a machine without a bank
' and shows what would fire. Replace those lines with the codes your
' controller uses to switch the bank on, select drills and switch it off.
'
' A post without SETUP_DRILL_BANK, FINAL_DRILL_BANK_ACTION and one way of
' selecting drills refuses Drill Bank toolpaths, so they are never sent to
' a machine that cannot select drills.

' History
' Who          When        What
' ===========  ==========  ==========================================
' CarveStudio  2026-09-26  First version.
' CarveStudio  2026-09-26  The drill bank block documented for setup.
' CarveStudio  2026-09-27  Drill bank setup, pattern and final sections.
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
' CarveStudio  2026-09-28  The spindle speed and start are their own SPINDLE_ON
'                          block, written after the tool change as before and
'                          again wherever a merged toolpath changes speed.

POST_NAME = "Generic G code with drill bank (mm) (*.gcode)"
FILE_EXTENSION = "gcode"
UNITS = MM
RAPID_RATE = 3000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"
LINE_NUMBER_START = 10
LINE_NUMBER_INCREMENT = 10

VAR LINE_NUMBER = [N|AF|N|1.0]
VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR Z_POSITION = [Z|A|Z|1.3]
VAR SAFE_HEIGHT = [SAFEZ|AF|Z|1.3]
VAR CLEAR_HEIGHT = [ZCLEAR|AF|Z|1.3]
VAR HOME_Z = [ZH|AF|Z|1.3]
VAR HOME_X = [XH|AF|X|1.3]
VAR HOME_Y = [YH|AF|Y|1.3]
VAR FEED_RATE = [F|AF|F|1.1]
VAR SPINDLE_SPEED = [S|AF|S|1.0]
VAR TOOL_NUMBER = [T|AF|T|1.0]
VAR ARC_CENTRE_X = [I|IF|I|1.3]
VAR ARC_CENTRE_Y = [J|IF|J|1.3]
VAR DWELL_TIME = [DWELL|AF|P|1.2]
VAR DRILL = [DB|A||1.0]
VAR DRILL_X = [DBX|A||1.0]
VAR DRILL_Y = [DBY|A||1.0]
VAR DRILL_LIST = [DL||,|,]

begin HEADER
+ "([TP_FILENAME])"
+ "(Written by CarveStudio)"
+ "([TOOLS_USED])"
+ "[N]G90"
+ "[N]G21"
+ "[N]G17"
+ "[N]G40"
end

begin TOOLCHANGE
+ "[N]M5"
+ "[N][T]M6"
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

' Written once as a drill bank toolpath starts. Switch the bank on here.
begin SETUP_DRILL_BANK
+ "(DRILL BANK ON)"
end

' Written after the rapid over a firing whose drills differ from the set
' already down, before its plunge. Replace the comment with your codes.
begin DRILL_BANK_PATTERN
+ "(DRILLS [DL] PATTERN [DB])"
end

' Written once as a drill bank toolpath ends. Raise the drills and switch
' the bank off here.
begin FINAL_DRILL_BANK_ACTION
+ "(DRILL BANK OFF)"
end

begin FOOTER
+ "[N]M5"
+ "[N]G0[ZCLEAR]"
+ "[N]G0[XH][YH]"
+ "[N]M30"
end
