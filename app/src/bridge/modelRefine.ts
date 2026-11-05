// Shapes of the Model menu's refinement commands (Smooth, Emboss, Scale
// Model Z Height, Offset Model, Add Draft, Component from Toolpath Preview).
// Lengths are millimetres, amounts percent.
//
// Forms that preview live pass the revision their last call answered as
// `replace`; the kernel then takes that step back before making the new
// one, so a form leaves a single undo step.

export interface SmoothComponentArgs {
  id: number;
  /** 0 to 100. */
  amount: number;
  preserveTransparency: boolean;
  replace?: number;
}

export interface EmbossComponentArgs {
  id: number;
  /** Height kept, above 0 and at most 100. */
  scale: number;
  /** 0 to 100. */
  detailSmoothing: number;
  replace?: number;
}

/** Either a percent of the model height or an exact height in mm. */
export interface ScaleHeightArgs {
  percent?: number;
  height?: number;
  /** A double sided job scales the other side's model too. */
  bothSides?: boolean;
  replace?: number;
}

export interface ScaleHeightReply {
  factor: number;
  maxHeight: number;
}

export interface OffsetModelArgs {
  distance: number;
  clipToZero: boolean;
  /** The previous preview this one adds onto. */
  merge?: { revision: number; id: number };
}

/** Which part of the toolpath preview becomes the component. */
export type PreviewComponentMode = 'whole' | 'overModel' | 'leftOver';

/** What Replace Below puts where the model is lower than the height. */
export type ReplaceWith = 'transparent' | 'plane';

/** model.replaceBelow: the components `ids`, else the visible components
 * of `level`; one is replaced in place, several are baked into one first. */
export interface ReplaceBelowArgs {
  ids?: number[];
  level?: number;
  /** Mm, as the chosen components show on their own. */
  height: number;
  with: ReplaceWith;
  /** Drop what is left by the height. */
  lower: boolean;
}
