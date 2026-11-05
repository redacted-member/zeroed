// The Create Inlay Toolpath chooser: the four kinds of straight walled
// inlay, each calculated by its own Profile or Pocket style form, and the
// kind an existing inlay toolpath was made as.

import type { StraightInlayParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';

const text = strings.toolpaths.straightInlay;

/** Male Straight, Male Stepped, Female Pocket and Female Hole. */
export type InlayKind = 'maleStraight' | 'maleStepped' | 'femalePocket' | 'femaleHole';

/** The chooser's buttons, grouped. */
export const INLAY_GROUPS: ReadonlyArray<{ label: string; kinds: readonly InlayKind[] }> = [
  { label: text.maleGroup, kinds: ['maleStraight', 'maleStepped'] },
  { label: text.femaleGroup, kinds: ['femalePocket', 'femaleHole'] },
];

export const KIND_LABELS: Record<InlayKind, string> = {
  maleStraight: text.straight,
  maleStepped: text.stepped,
  femalePocket: text.pocket,
  femaleHole: text.hole,
};

/** The form title of each kind. */
export const KIND_TITLES: Record<InlayKind, string> = {
  maleStraight: text.maleTitle,
  maleStepped: text.maleSteppedTitle,
  femalePocket: text.femalePocketTitle,
  femaleHole: text.femaleHoleTitle,
};

/** The kind an inlay toolpath was calculated as. */
export function kindOf(params: Pick<StraightInlayParams, 'part' | 'male' | 'female'>): InlayKind {
  if (params.part === 'male') {
    return params.male === 'stepped' ? 'maleStepped' : 'maleStraight';
  }
  return params.female === 'hole' ? 'femaleHole' : 'femalePocket';
}

/** The kernel's part and shape settings of a kind. */
export function kindParams(kind: InlayKind): Pick<StraightInlayParams, 'part' | 'male' | 'female'> {
  switch (kind) {
    case 'maleStraight':
      return { part: 'male', male: 'straight', female: 'pocket' };
    case 'maleStepped':
      return { part: 'male', male: 'stepped', female: 'pocket' };
    case 'femalePocket':
      return { part: 'female', male: 'straight', female: 'pocket' };
    case 'femaleHole':
      return { part: 'female', male: 'straight', female: 'hole' };
  }
}

/** The name new toolpaths of a kind count up from. */
export function baseNameOf(kind: InlayKind): string {
  return kind === 'maleStraight' || kind === 'maleStepped' ? text.maleName : text.femaleName;
}
