# Machine checklist: before trusting Zeroed output on real work

Software tests can prove the G-code matches the design. Only the machine can prove the design, the post and the
machine agree. Work through these once per machine and post, and again after any post change.

## 1. Before anything moves
- Machine Configuration: the machine's post is selected, units match the controller (mm or inch), and the
  bed size matches.
- Tool Database: the tools you will use have their real diameter, flute length, shank diameter and holder size
  (the preview's collision checks use them), feeds and speeds you trust, and the right tool numbers.
- Job Setup: material thickness measured with calipers at several points (not the nominal size), Z zero on the
  material top or the bed as you will set it on the machine, XY datum where you will touch off.

## 2. On screen
- Preview Toolpaths at High or better: Checks show no collisions, no gouges, no rapids through material.
- The toolpath summary's depths are what you expect; Save Toolpaths writes one file per tool if the machine has
  no tool changer.
- Open the saved file in a separate G-code viewer (your controller's own preview, or any viewer you trust) and
  compare the shape and depths with Zeroed's preview.

## 3. Air cut a tool change first
Before the first air cut of a job, run a program with one tool change and nothing else under the spindle (save
two short toolpaths with different tools), spindle off or at low speed, hand on the stop button. On LinuxCNC and
UCCNC pick the post form that matches the machine (README, "Tool changes on LinuxCNC and UCCNC"):
- "M6 probes tool length": the spindle stops and lifts clear, the message names the tool, M6 runs your probing
  routine (change position, wait, probe). After it the program must lift straight up and only then move across.
  Then check the length took: jog the new tool down to the material top (or a known block) and read Z; it should
  read the height you zeroed at, within the probe's repeatability. On LinuxCNC check that the tool length offset
  in the status display changed to the new tool's (the remap applies it, the program writes no G43 H). If Z is off
  by a whole tool length, the routine did not apply the length: use the manual form or add `G43H[T]` to a copy of
  the post.
- "manual touch off at M0": the spindle stops and lifts clear, the message names the tool, the program stops at
  M0; fit and touch off the tool, resume, and it must lift straight up before it moves across.
- Either form: the first tool of the file goes through the same change. If the routine skips the change when the
  tool asked for is already in the spindle, Z zero must already be set for that tool.

## 4. Air cut (no material, or Z zero set 25 mm above the stock)
- Spindle on, feed override at 50%, hand on the stop button.
- Watch: the first move goes to safe Z before XY; plunges happen where the preview shows them; tabs lift the tool;
  tool changes stop and ask; the job ends at home Z, clear of the work.

## 5. Scrap cut
- A small test of each toolpath type you use: pocket to a measured depth, profile with tabs, V-groove text, a drill
  hole with pecks.
- Measure with calipers: pocket depth, profile size (inside and outside), hole diameter and depth. They should be
  within your machine's accuracy of the design.
- Joints: make the Joint Creator's Test Coupon for the fit you use (Press, Slip, Glue), cut it in the real
  material, and check the fit by hand. If it is tight or loose, change the fit's clearance and cut again before
  cutting parts. Record the working clearance per material.

## 6. Record
- Note the post, controller, material, tool and the fit clearances that worked, so the next job starts from them.
- If anything differs from the preview, save the job and the G-code and report it: that is a bug to fix before
  the next real job.

## Rotary jobs: once per machine and rotary post
Do this with no blank in the chuck first, then with a blank of scrap.
- Machine Configuration, Rotary Axis: the letter, units and direction match the controller (A, Degrees, Normal until
  you change them); Save Toolpaths says what the file will turn.
- Set up the controller: A is an angular axis in degrees with steps per degree calibrated. Command `G0 A360`, mark
  the chuck, and check it comes back to the mark. A short turn with a dial or a pointer proves the scale.
- Set the zeros: the wrapped linear axis (Y on a blank along X, X on a blank along Y) on the rotary axis's center,
  A zero wherever the design's Y 0 (or X 0) should land, and Z zero as Job Setup says: on the rotary center or on
  the top of the blank. Measure the real diameter and type it in Job Setup.
- Save with a rotary post (Save Toolpaths names the axis it turns) and do an air cut (Trial Run). Watch it lift Z
  before it turns or crosses, at the file start and after each tool change.
- Probing M6: run one tool change on the air cut. The file writes G94 before M6 and G93 after it, so the routine's
  feeds are ordinary feeds. Check the routine does not set its own feed mode and leave it.
- Direction: cut text or an arrow into scrap, a shallow scrap cut. Reads mirrored on the blank? The axis turns the
  other way from the toolpath: set Direction to Reversed in Machine Configuration, Rotary Axis, and save again.
- Size: wrap a square the job calls 1 x 1 in and measure it on the blank. Off around the blank but right along it
  means the diameter in Job Setup or the steps per degree are wrong.
- UCCNC: G93 needs a version with inverse time (it is shown running on V1.2113); check yours on the air cut.
