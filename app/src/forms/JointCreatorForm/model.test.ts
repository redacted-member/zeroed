import { describe, expect, it } from 'vitest';

import type { JointRow, PickedEdge } from '../../bridge/joints';
import {
  afterCreate,
  convertJointUnits,
  editState,
  fitLabel,
  FITS_ONE_WAY_BIAS,
  JOINT_KINDS,
  initialJointState,
  jointsOf,
  paramsOf,
  secondPickProblem,
  sentence,
  specOf,
  stage,
  stepBack,
  withKind,
  withOffset,
  kindInfo,
  shownLine,
} from './model';

function edge(vector: number, from: [number, number], to: [number, number]): PickedEdge {
  return {
    vector,
    from,
    to,
    length: Math.hypot(to[0] - from[0], to[1] - from[1]),
    sheet: 1,
    name: null,
    thickness: 12,
    joint: null,
  };
}

const A = edge(1, [110, 60], [10, 60]);
const B = edge(2, [10, 100], [110, 100]);

describe('joint creator model', () => {
  it('asks for the first edge, then the second, then is ready', () => {
    let state = initialJointState();
    expect(stage(state)).toBe('pickA');
    expect(specOf(state, 'mm')).toBeNull();
    state = { ...state, a: A };
    expect(stage(state)).toBe('pickB');
    state = { ...state, b: { kind: 'edge', edge: B } };
    expect(stage(state)).toBe('ready');
    const spec = specOf(state, 'mm');
    expect(spec).toEqual({
      kind: 'finger',
      a: { vector: 1, from: [110, 60], to: [10, 60] },
      b: { kind: 'edge', vector: 2, from: [10, 100], to: [110, 100] },
      params: {},
    });
  });

  it('steps back one pick at a time with Esc', () => {
    const ready = { ...initialJointState(), a: A, b: { kind: 'edge' as const, edge: B } };
    const one = stepBack(ready);
    expect(one.b).toBeNull();
    expect(one.a).toEqual(A);
    const none = stepBack(one);
    expect(none.a).toBeNull();
    expect(stepBack(none)).toBe(none);
  });

  it('refuses the same part twice', () => {
    const state = { ...initialJointState(), a: A };
    expect(secondPickProblem(state, 1)).toBe('samePart');
    expect(secondPickProblem(state, 2)).toBeNull();
  });

  it('sends only the sizes typed, in millimetres, and the options set', () => {
    const state = {
      ...initialJointState(),
      a: A,
      b: { kind: 'edge' as const, edge: B },
      numbers: { count: '7', depth: '0.25' },
      fitsOneWay: true,
      blind: true,
      start: 'gap' as const,
    };
    expect(paramsOf(state, 'mm').params).toEqual({
      count: 7,
      depth: 0.25,
      bias: FITS_ONE_WAY_BIAS,
      blind: true,
      start: 'gap',
    });
    // Inches: the depth converts, the count does not.
    expect(paramsOf(state, 'inch').params.depth).toBeCloseTo(6.35, 9);
    expect(paramsOf(state, 'inch').params.count).toBe(7);
  });

  it('flags bad sizes and holds the preview back', () => {
    const state = {
      ...initialJointState(),
      a: A,
      b: { kind: 'edge' as const, edge: B },
      numbers: { count: '2.5' },
    };
    expect(paramsOf(state, 'mm').errors.count).toBe('whole');
    expect(specOf(state, 'mm')).toBeNull();
    const negative = { ...state, numbers: { count: '-3' } };
    expect(paramsOf(negative, 'mm').errors.count).toBe('positive');
  });

  it('converts typed lengths when the job unit changes', () => {
    const state = { ...initialJointState(), numbers: { depth: '12.7', count: '5' } };
    const inch = convertJointUnits(state, 'mm', 'inch');
    expect(Number(inch.numbers.depth)).toBeCloseTo(0.5, 3);
    expect(inch.numbers.count).toBe('5');
  });

  it('keeps the picks that suit a new kind and clears its sizes', () => {
    const state = {
      ...initialJointState(),
      a: A,
      b: { kind: 'edge' as const, edge: B },
      numbers: { count: '9' },
    };
    const same = withKind(state, 'finger');
    expect(same.b).not.toBeNull();
    expect(same.numbers).toEqual({});
  });

  it('opens a stored joint for editing and starts over after Create', () => {
    const row: JointRow = {
      id: 4,
      kind: 'finger',
      a: { vector: 1, from: [110, 60], to: [10, 60] },
      b: { kind: 'edge', vector: 2, from: [10, 100], to: [110, 100] },
      params: { count: 7, bias: 0.3, depth: 8, blind: true },
      vectors: [],
      resolved: { count: 7 },
      parts: [1, 2],
      ok: true,
    };
    const state = editState(row, 'mm', initialJointState());
    expect(state.editing).toBe(4);
    expect(state.numbers).toEqual({ count: '7', depth: '8' });
    expect(state.fitsOneWay).toBe(true);
    expect(state.blind).toBe(true);
    expect(stage(state)).toBe('ready');
    expect(specOf(state, 'mm')?.params).toEqual({
      count: 7,
      depth: 8,
      bias: FITS_ONE_WAY_BIAS,
      blind: true,
    });
    const next = afterCreate(state);
    expect(stage(next)).toBe('pickA');
    expect(next.editing).toBeNull();
    expect(next.kind).toBe('finger');
    expect(jointsOf([row], 2)).toEqual([row]);
    expect(jointsOf([row], 3)).toEqual([]);
  });

  it('turns kernel messages into sentences', () => {
    expect(sentence('pick edges of the same length')).toBe('Pick edges of the same length.');
    expect(sentence('Done.')).toBe('Done.');
    expect(sentence('a part is shorter than the other part is wide; pick longer parts')).toBe(
      'A part is shorter than the other part is wide; pick longer parts.',
    );
  });

  it('makes a box from the selection in its own stage', () => {
    const state = { ...initialJointState(), box: [1, 2, 3, 4, 5] };
    expect(stage(state)).toBe('box');
    expect(stepBack(state).box).toBeNull();
  });

  it('sends a face pick for a dado and moves it by the typed offset', () => {
    const base = edge(3, [10, 10], [210, 10]);
    let state = withKind({ ...initialJointState(), a: A }, 'dado');
    expect(kindInfo(state.kind).target).toBe('face');
    state = { ...state, b: { kind: 'face', edge: base, offset: 38.2, along: 100 } };
    state = withOffset(state, 40);
    expect(specOf(state, 'mm')?.b).toEqual({
      kind: 'face',
      edge: { vector: 3, from: [10, 10], to: [210, 10] },
      offset: 40,
      along: 100,
    });
    // An edge pick does not suit a dado; switching kinds drops it.
    const fromFinger = withKind(
      { ...initialJointState(), a: A, b: { kind: 'edge', edge: B } },
      'dado',
    );
    expect(fromFinger.b).toBeNull();
    expect(stage(fromFinger)).toBe('pickB');
  });

  it('sends round tab ends and a tenon that is blind', () => {
    const tabs = {
      ...withKind(initialJointState(), 'tabSlot'),
      numbers: { count: '3', proud: '6' },
      ends: 'rounded' as const,
      fitsOneWay: true,
      blind: true,
    };
    // Blind is not a tab and slot option, so it is not sent.
    expect(paramsOf(tabs, 'mm').params).toEqual({
      count: 3,
      proud: 6,
      bias: FITS_ONE_WAY_BIAS,
      ends: 'rounded',
    });
    const tenon = { ...withKind(initialJointState(), 'mortiseTenon'), blind: true };
    expect(paramsOf(tenon, 'mm').params).toEqual({ blind: true });
  });

  it('sends the places clicked on the edges for a cross lap unless typed', () => {
    const a = { ...A, at: 150 };
    const b = { ...B, at: 50 };
    const state = {
      ...withKind(initialJointState(), 'crossLap'),
      a,
      b: { kind: 'edge' as const, edge: b },
    };
    expect(specOf(state, 'mm')?.params).toEqual({ atA: 150, atB: 50 });
    const typed = { ...state, numbers: { atB: '60' } };
    expect(specOf(typed, 'mm')?.params).toEqual({ atA: 150, atB: 60 });
  });

  it('sends pegs right through as not blind', () => {
    const pegs = { ...withKind(initialJointState(), 'dowel'), through: true };
    expect(paramsOf(pegs, 'mm').params).toEqual({ blind: false });
    const dowels = withKind(initialJointState(), 'dowel');
    expect(paramsOf(dowels, 'mm').params).toEqual({});
  });

  it('makes a living hinge from one pick on one part', () => {
    const state = { ...withKind(initialJointState(), 'livingHinge'), a: { ...A, at: 40 } };
    expect(stage(state)).toBe('ready');
    expect(specOf(state, 'mm')).toEqual({
      kind: 'livingHinge',
      a: { vector: 1, from: [110, 60], to: [10, 60] },
      b: { kind: 'edge', vector: 1, from: [110, 60], to: [10, 60] },
      params: { atA: 40 },
    });
  });

  it('sends a hinge pattern other than straight and keeps it when editing', () => {
    const hinge = withKind({ ...initialJointState(), a: { ...A, at: 50 } }, 'livingHinge');
    expect(specOf(hinge, 'mm')?.params.pattern).toBeUndefined();
    const spiral = { ...hinge, pattern: 'spiral' as const, numbers: { radius: '30' } };
    const spec = specOf(spiral, 'mm');
    expect(spec?.params).toEqual({ pattern: 'spiral', radius: 30, atA: 50 });
    const row: JointRow = {
      ...(spec as NonNullable<typeof spec>),
      id: 4,
      vectors: [],
      resolved: {},
      parts: [1, 1],
      ok: true,
    };
    expect(editState(row, 'mm', initialJointState()).pattern).toBe('spiral');
  });

  it('offers the V joint on two edges with its angle and the twist lock on a face', () => {
    expect(kindInfo('vJoint').target).toBe('edge');
    expect(kindInfo('vJoint').numbers.find((n) => n.essential === true)?.key).toBe('angle');
    expect(kindInfo('twistLock').target).toBe('face');
    expect(kindInfo('cornerDovetail').target).toBe('edge');
    // The kerf tenon stands on the second part's face; its width is the
    // size that matters, the kerf depth under More.
    const kerf = kindInfo('kerfTenon');
    expect(kerf.kind).toBe('kerfTenon');
    expect(kerf.target).toBe('face');
    expect(kerf.numbers.find((n) => n.essential === true)?.key).toBe('width');
    expect(kerf.numbers.find((n) => n.key === 'depth')?.label).toBe('kerfDepth');
    const v = {
      ...withKind(initialJointState(), 'vJoint'),
      a: A,
      b: { kind: 'edge' as const, edge: B },
    };
    expect(specOf({ ...v, numbers: { angle: '120' } }, 'inch')?.params.angle).toBe(120);
  });
});

describe('joint creator panel text', () => {
  it('names each fit with its clearance a side in the job unit', () => {
    expect(fitLabel('slip', 'mm')).toBe('Slip (0.2 mm a side)');
    expect(fitLabel('knockDown', 'mm')).toBe('Knock-down (0.25 mm a side)');
    // 0.2 mm is 0.007874 inch, shown to four places.
    expect(fitLabel('slip', 'inch')).toBe('Slip (0.0079 inch a side)');
    expect(fitLabel('press', 'inch')).toBe('Press (0.0039 inch a side)');
    expect(fitLabel('custom', 'inch')).toBe('Custom');
  });

  it('orders the gallery by how the parts meet, finger first', () => {
    expect(JOINT_KINDS.map((k) => k.kind)).toEqual([
      'finger',
      'cornerDovetail',
      'rabbet',
      'vJoint',
      'dado',
      'tabSlot',
      'mortiseTenon',
      'wedgedTenon',
      'kerfTenon',
      'dowel',
      'captiveNut',
      'twistLock',
      'camLock',
      'groove',
      'screws',
      'dovetail',
      'butterfly',
      'puzzle',
      'crossLap',
      'halfLap',
      'livingHinge',
    ]);
  });

  it('shows one line: what was just made until the next pick, else what to click', () => {
    expect(shownLine('Made. Click the next edge, or pick another joint.', 'Click the edge.')).toBe(
      'Made. Click the next edge, or pick another joint.',
    );
    expect(shownLine(null, 'Click the edge of the first part.')).toBe(
      'Click the edge of the first part.',
    );
  });
});
