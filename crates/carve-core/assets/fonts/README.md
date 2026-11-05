# Bundled fonts

## Fallback outline font

These faces are the fallback the kernel uses when the requested font family
is not installed on the machine. They are Fira Sans, licensed under the SIL
Open Font License version 1.1, a copy of which is in `OFL.txt` next to them.

Copyright (c) 2012-2015, The Mozilla Foundation and Telefonica S.A.

The files here are subsets of the originals, cut down to the Latin ranges the
application needs so the bundle stays small. Fira Sans carries no Reserved
Font Name, so the subsets keep the original family name. Nothing else about
the faces was changed. The original full faces are published at
https://github.com/mozilla/Fira and are also distributed through Google Fonts.

Covered: U+0020 to U+007E, U+00A0 to U+00FF, curly quotes, the ellipsis, en
and em dash, degree, multiplication sign, euro, pound, yen, copyright,
registered and trade mark signs.

## Single line (engraving) fonts

`SingleLineSans.txt` and `SingleLineScript.txt` are single line fonts: each
letter is a few open strokes a cutter follows once. They are converted from
the public domain Hershey font data, the Roman Simplex (`futural.jhf`) and
Script Simplex (`scripts.jhf`) sets, as distributed at
http://emergent.unpythonic.net/software/hershey and mirrored in
https://github.com/kamalmostafa/hershey-fonts.

Acknowledgements the Hershey data asks to be distributed with it:

- The Hershey Fonts were originally created by Dr. A. V. Hershey while
  working at the U. S. National Bureau of Standards.
- The format of the Font data in this distribution was originally created by
  James Hurt, Cognition, Inc., 900 Technology Park Drive, Billerica, MA 01821.

The data was converted into our own plain text format (not the NTIS
format): a header with the family name and metrics (`em`, `ascender`,
`descender`, `cap`), then one `glyph` line per character, U+0020 to U+007E,
giving its code point in hex, its advance and its strokes. Strokes are
separated by `;` and each is a list of `x,y` pairs; y runs up from the
baseline and the cap height is 21 units. The glyph shapes are unchanged.
The kernel reads the files in `crates/carve-core/src/geom/text/stroke.rs`.
