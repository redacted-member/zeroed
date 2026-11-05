# Contributing to Zeroed

Bug reports, fixes and features are all welcome. If you fixed something in your
own copy, please send it back so everyone gets it.

## Reporting a bug

Open an issue with what you did, what you expected and what happened. For wrong
G-code, include the post processor, your controller and the lines that are wrong.
A saved `.carve` file that shows the problem helps most.

## Sending a change

1. Fork the repo and make a branch.
2. Write a failing test first: a Rust unit test beside the code for the kernel, a
   vitest test of the form or panel model for the interface. Tests should check
   exact values.
3. Make the change.
4. Run the whole check and read its output. It must print `GATE PASSED`:

   ```bash
   tools/agents/gate.sh $PWD
   ```

5. Open a pull request saying what changed and how you checked it.

## Conventions

- Rust kernel, React and TypeScript interface, Sass per component. No utility CSS
  frameworks, no UI kits, no inline styles.
- Every UI string lives in `app/src/lib/strings`. US spelling, no em dashes.
- Whole, even pixel values in CSS.
- The kernel works in millimeters. Convert only at the point of display.

## Machine safety

Any change to a post or to toolpath output must keep `tools/gcode-check` passing.
Z must lift before any XY move at the file start and after every tool change. Run
new output as an air cut on a real machine before trusting it, and say so in your
pull request if you could not.

## License

By contributing, you agree that your work is released under the
[GNU AGPL, version 3 or later](LICENSE), the same license as the rest of the code.
