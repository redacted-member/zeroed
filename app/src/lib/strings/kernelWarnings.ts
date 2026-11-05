// Warnings the kernel reports by code (toolpath calculation, vector and
// toolpath import). lib/kernelWarnings.ts picks the sentence for a code and
// fills in its values; a code with no entry here shows the kernel's English.

export const kernelWarningsStrings = {
  kernelWarnings: {
    length: (mm: string) => `${mm} mm`,
    chamferMultiPass: (reach: string) =>
      `The chamfer is wider than the tool can cut in one go (${reach}), so it is cut in several passes stepping out.`,
    chamferPassesLeftOut: 'Some chamfer passes did not fit inside their vectors and were left out.',
    prismDeeperThanCone: (depth: string) =>
      `The prism is deeper than the tool's cone reaches (${depth}), so the straight sides of the tool cut above the cone.`,
    prismTopFlat: 'The prism is taller than the maximum depth allows, so its top is left flat.',
    threadPitchDiffers: (toolPitch: string, pitch: string) =>
      `The tool's pitch (${toolPitch}) differs from the thread's (${pitch}); only a single point cutter can cut it.`,
    toolTooBig: 'The tool is too big to fit inside some vectors; they were left out.',
    mouldingPassLeftOut: (offset: string) =>
      `The rail has no room for the pass ${offset} across; it is left out.`,
    mouldingNoRoom: 'The rail has no room for any pass across the molding.',
    nothingLeft: (tool: string) => `${tool} has nothing left to machine after the tools before it.`,
    noSmallerTool: (tool: string) =>
      `${tool} is no smaller than the clearance tool before it, so it has nothing left to clear.`,
    keepOut: (zone: string) =>
      `The tool goes into the keep-out zone on layer ${zone}: move the clamps or change the toolpath before cutting.`,
    drillOpenVectors: 'Open vectors have no center to drill and were left out.',
    textureNoStrokes: 'No strokes fit inside the vectors.',
    flutingRampShortened:
      'The ramp length is longer than some vectors allow; those ramps were shortened to fit.',
    pocketStepoverWide: 'The stepover is wider than the tool, so ridges will be left standing.',
    cutsThrough: (depth: string, thickness: string) =>
      `The toolpath cuts ${depth} deep, right through the ${thickness} material. Set a flat depth or a thicker material if that is not meant.`,
    vectorsRepaired: (count: number) =>
      count === 1
        ? '1 vector crossed itself and was repaired before machining.'
        : `${count} vectors crossed themselves and were repaired before machining.`,
    fixingTimedOut: (seconds: number) =>
      `Geometry fixing stopped after ${seconds} s (Preferences, Toolpaths): the remaining vectors are machined as drawn.`,
    inlayBoundaryTooSmall:
      'The boundary offset is too small for the plug walls to reach the plug depth all round; the plug will be shorter there.',
    flatsNeedPocket:
      'Some areas are wider than the flat depth allows; their middles are left at the flat depth and need a pocket to clear them.',
    gcodeSkippedArcPlanes: 'arcs outside the XY plane (G18, G19)',
    gcodeSkippedRotary: 'rotary axes (A, B, C)',
    importTextSkipped: 'text was skipped: import it as vectors from the source program',
    importImageSkipped: 'a placed image was skipped',
    importUseSkipped: 'a reused shape was skipped',
    importBlockMissing: 'a block reference pointed at a missing block',
    importBlockTooDeep: 'block references were nested too deeply to follow',
    importFontSubstituted:
      'some text uses a font the file does not include; an installed font was used',
  },
} as const;
