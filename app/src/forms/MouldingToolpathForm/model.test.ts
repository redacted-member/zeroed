import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import type { EntitySummary, Path } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import {
  cutDepth,
  initialMouldingState,
  mouldingArgs,
  mouldingPreview,
  mouldingStateFrom,
  otherGap,
  profileHeight,
  railsAndProfile,
  validateMoulding,
  withPosition,
  withSlider,
  type MouldingContext,
} from './model';

const text = strings.toolpaths.moulding;

function entity(id: number, kind: EntitySummary['kind'], height = 1): EntitySummary {
  return {
    id,
    kind,
    layer: 0,
    bbox: [0, 0, 10, height],
    closed: false,
    revision: 1,
  } as EntitySummary;
}

/** Rails 1 and 2, profile 7 (5 high) in 20 of material. */
const context: MouldingContext = { rails: [1, 2], profile: 7, thickness: 20, profileHeight: 5 };

const mill = { id: 4, name: 'End Mill 12 mm', toolType: { kind: 'endMill' } } as unknown as Tool;

describe('moulding toolpath form model', () => {
  it('reads the rails and then the profile from the selection order', () => {
    const entities = [
      entity(1, 'path'),
      entity(2, 'path'),
      entity(3, 'text'),
      entity(7, 'path', 5),
    ];
    expect(railsAndProfile([1, 2, 7], entities)).toEqual({ rails: [1, 2], profile: 7 });
    expect(railsAndProfile([7, 3, 1], entities)).toEqual({ rails: [7], profile: 1 });
    expect(railsAndProfile([1], entities)).toEqual({ rails: [1], profile: null });
    expect(railsAndProfile([], entities)).toEqual({ rails: [], profile: null });
    expect(profileHeight(7, entities)).toBe(5);
    expect(profileHeight(null, entities)).toBeNull();
  });

  it('needs a rail and a profile unless the rails are picked automatically', () => {
    const state = initialMouldingState('mm');
    const none = { ...context, rails: [], profile: null };
    expect(validateMoulding(state, 'mm', none).rail).toBe(text.needsRail);
    expect(validateMoulding(state, 'mm', none).profile).toBe(text.needsProfile);
    expect(mouldingArgs(state, 'mm', none)).toBeNull();
    const automatic = { ...none, automatic: true };
    expect(validateMoulding(state, 'mm', automatic)).toEqual({});
    expect(mouldingArgs(state, 'mm', automatic)?.ids).toEqual([]);
  });

  it('sends the rails then the profile, and the defaults', () => {
    expect(mouldingArgs(initialMouldingState('mm'), 'mm', context)).toEqual({
      ids: [1, 2, 7],
      params: {
        stepover: null,
        widthScale: 1,
        heightScale: 1,
        allowance: 0,
        strategy: 'along',
        startDepth: 0,
        boundaryOffset: 0,
        autoBoundaryOffset: true,
        varyStepover: true,
        sharpCorners: false,
        clearanceTool: null,
        clearanceAllowance: 0,
        clearanceRamp: { enabled: false, distance: 10 },
        machineFlatRegions: false,
        skipFlatRegions: false,
      },
    });
  });

  it('places the toolpath by the gap above or below and shows the cut depth', () => {
    const state = { ...initialMouldingState('mm'), gapAbove: '3' };
    expect(otherGap(state, 'mm', context)).toBe('12');
    expect(cutDepth(state, 'mm', context)).toBe(8);
    const below = withPosition(state, 'below', 'mm', context);
    expect(below).toMatchObject({ position: 'below', gapBelow: '12' });
    expect(mouldingArgs({ ...below, gapBelow: '10' }, 'mm', context)?.params.startDepth).toBe(5);
    expect(withSlider(below, 4, 'mm', context).gapBelow).toBe('11');
    expect(withSlider(state, 4, 'mm', context).gapAbove).toBe('4');
    expect(validateMoulding({ ...below, gapBelow: '16' }, 'mm', context).gapBelow).toBe(
      text.tooDeep,
    );
    expect(validateMoulding({ ...state, gapAbove: '-1' }, 'mm', context).gapAbove).toBeDefined();
  });

  it('keeps settings of older toolpaths the form no longer shows', () => {
    const args = mouldingArgs(initialMouldingState('mm'), 'mm', context);
    expect(args).not.toBeNull();
    if (args === null) {
      return;
    }
    const old = { ...args.params, strategy: 'across' as const, heightScale: 1.2, allowance: -0.5 };
    const back = mouldingStateFrom(old, 'M', 'mm');
    const again = mouldingArgs(back, 'mm', context);
    expect(again?.params).toMatchObject({ strategy: 'across', heightScale: 1.2, allowance: -0.5 });
    // Older toolpaths had no automatic boundary offset.
    expect(
      mouldingStateFrom({ ...old, autoBoundaryOffset: undefined }, 'M', 'mm').autoBoundaryOffset,
    ).toBe(false);
  });

  it('sends the clearance tool with its flats and allowance only while it is used', () => {
    const state = {
      ...initialMouldingState('mm'),
      useClearance: true,
      clearanceTool: mill,
      machineFlats: true,
      skipFlats: true,
      clearanceAllowance: '1',
      clearanceRamp: true,
      clearanceRampDistance: '15',
    };
    const args = mouldingArgs(state, 'mm', context);
    expect(args?.params).toMatchObject({
      machineFlatRegions: true,
      skipFlatRegions: true,
      clearanceAllowance: 1,
      clearanceRamp: { enabled: true, distance: 15 },
    });
    expect(args?.params.clearanceTool?.id).toBe(4);
    if (args !== null) {
      const back = mouldingStateFrom(args.params, 'M', 'mm');
      expect(back).toMatchObject({ useClearance: true, machineFlats: true, skipFlats: true });
    }
    const off = mouldingArgs({ ...state, useClearance: false }, 'mm', context);
    expect(off?.params).toMatchObject({
      clearanceTool: null,
      machineFlatRegions: false,
      skipFlatRegions: false,
    });
    expect(
      validateMoulding({ ...state, clearanceAllowance: '-1' }, 'mm', context).clearanceAllowance,
    ).toBeDefined();
  });

  it('sends a typed boundary offset only without the automatic one, and the profile filter', () => {
    const state = { ...initialMouldingState('mm'), autoBoundaryOffset: false, boundaryOffset: '3' };
    expect(mouldingArgs(state, 'mm', context)?.params).toMatchObject({
      boundaryOffset: 3,
      autoBoundaryOffset: false,
    });
    expect(
      validateMoulding({ ...state, boundaryOffset: '-1' }, 'mm', context).boundaryOffset,
    ).toBeDefined();
    const filter = { open: true, closed: false, associate: true } as never;
    const picked = mouldingArgs({ ...state, profileSelector: filter }, 'mm', context);
    expect(picked?.params.profileSelector).toBe(filter);
  });

  it('previews the rails and the profile that are loaded', () => {
    const path = { subpaths: [] } as unknown as Path;
    expect(mouldingPreview(context, new Map([[7, path]]))).toEqual([path]);
    expect(mouldingPreview({ ...context, rails: [], profile: null }, new Map([[7, path]]))).toEqual(
      [],
    );
  });
});
