import { describe, expect, it } from 'vitest';

import type { ProfileParams } from '../../bridge/cam';
import { DEFAULT_JOB } from '../../bridge/types';
import {
  initialProfileState,
  newProfileState,
  leadArcLength,
  profileParams,
  profileThroughDepth,
  rampFields,
  toggleOrder,
  profileStateFrom,
  tabSpec,
  spiralAllowed,
  validateProfile,
} from './model';

describe('profile toolpath form model', () => {
  it('starts a flat job at its thickness and a rotary job at the radius of its blank', () => {
    expect(profileThroughDepth({ ...DEFAULT_JOB, thickness: 12 })).toBe(12);
    const rotary = {
      ...DEFAULT_JOB,
      kind: 'rotary' as const,
      thickness: 30,
      rotary: { diameter: 60, axis: 'x' as const },
    };
    expect(profileThroughDepth(rotary)).toBe(30);
    const cut = profileParams(initialProfileState('mm', profileThroughDepth(rotary)), 'mm');
    expect(cut?.cutDepth).toBe(30);
  });

  it('starts cutting through the material when its thickness is known', () => {
    expect(profileParams(initialProfileState('mm', 12), 'mm')?.cutDepth).toBe(12);
    expect(profileParams(initialProfileState('inch', 19.05), 'inch')?.cutDepth).toBeCloseTo(
      19.05,
      6,
    );
    expect(profileParams(initialProfileState('mm', 0), 'mm')?.cutDepth).toBe(18);
  });

  it('cuts outside, climb, to the full default depth', () => {
    expect(profileParams(initialProfileState('mm'), 'mm')).toEqual({
      startDepth: 0,
      cutDepth: 18,
      passDepth: null,
      passCount: null,
      lastPassThickness: null,
      exactPassDepth: false,
      passList: null,
      side: 'outside',
      openSide: 'right',
      direction: 'climb',
      allowance: 0,
      lastPass: { enabled: false, allowance: 0, reverse: false },
      tabs: {
        enabled: false,
        count: 2,
        length: 10,
        thickness: 3,
        triangular: false,
        spacing: null,
        placement: 'avoidCorners',
      },
      tabPositions: null,
      tabAnchors: null,
      ramp: { enabled: false, distance: 20, kind: 'smooth', angle: null, onLeadIn: false },
      lead: {
        kind: 'none',
        length: 5,
        angleDeg: 45,
        radius: 5,
        arcAngleDeg: 90,
        overcut: 0,
        leadOut: true,
      },
      order: 'optimized',
      orders: [],
      startAt: 'optimized',
      startAnchor: 'bottomLeft',
      sharpCorners: false,
      sharpInternalCorners: false,
      wallAngle: 0,
      scallop: 0.02,
    });
  });

  it('asks for a tapered wall by its angle, with a scallop only when it leans', () => {
    const upright = initialProfileState('mm');
    expect(validateProfile({ ...upright, scallop: '0' }, 'mm')).toEqual({});
    const leaning = { ...upright, wallAngle: '14', scallop: '0.01' };
    expect(profileParams(leaning, 'mm')).toMatchObject({ wallAngle: 14, scallop: 0.01 });
    expect(Object.keys(validateProfile({ ...leaning, scallop: '0' }, 'mm'))).toEqual(['scallop']);
    expect(Object.keys(validateProfile({ ...leaning, wallAngle: '50' }, 'mm'))).toEqual([
      'wallAngle',
    ]);
    // Inches: the scallop is shown in inches and sent in millimeters.
    const inch = { ...initialProfileState('inch'), wallAngle: '10', scallop: '0.001' };
    expect(profileParams(inch, 'inch')?.scallop).toBeCloseTo(0.0254, 9);
  });

  it('keeps a Joint Creator socket wall rigid when it is recalculated', () => {
    const params = {
      ...profileParams(initialProfileState('mm'), 'mm'),
      wallAngle: 14,
      scallop: 0.02,
      rigid: true,
    } as ProfileParams;
    const back = profileStateFrom(params, 'Joints tapered walls', 'mm');
    expect(back.wallAngle).toBe('14');
    expect(profileParams(back, 'mm')).toMatchObject({ wallAngle: 14, scallop: 0.02, rigid: true });
    const plain = profileParams(initialProfileState('mm'), 'mm');
    expect(plain !== null && 'rigid' in plain).toBe(false);
  });

  it('checks only the lead fields the chosen lead uses', () => {
    const bad = {
      ...initialProfileState('mm'),
      leadLength: '0',
      leadAngle: '120',
      leadRadius: '-1',
      leadOvercut: '-2',
    };
    expect(validateProfile(bad, 'mm')).toEqual({});
    expect(Object.keys(validateProfile({ ...bad, leadKind: 'straight' }, 'mm')).sort()).toEqual([
      'leadAngle',
      'leadLength',
      'leadOvercut',
    ]);
    expect(Object.keys(validateProfile({ ...bad, leadKind: 'arc' }, 'mm')).sort()).toEqual([
      'leadOvercut',
      'leadRadius',
    ]);
    expect(profileParams({ ...bad, leadKind: 'arc' }, 'mm')).toBeNull();
  });

  it('sends the lead in millimetres', () => {
    const state = {
      ...initialProfileState('inch'),
      leadKind: 'arc' as const,
      leadRadius: '0.25',
      leadOvercut: '0.1',
    };
    expect(profileParams(state, 'inch')?.lead).toEqual({
      kind: 'arc',
      length: 5.08,
      angleDeg: 45,
      radius: 6.35,
      arcAngleDeg: 90,
      overcut: 2.54,
      leadOut: true,
    });
    const straight = {
      ...initialProfileState('mm'),
      leadKind: 'straight' as const,
      leadAngle: '90',
    };
    expect(profileParams(straight, 'mm')?.lead).toMatchObject({
      kind: 'straight',
      length: 5,
      angleDeg: 90,
    });
  });

  it('checks tab and ramp fields only when they are switched on', () => {
    const state = { ...initialProfileState('mm'), tabCount: '0', rampDistance: 'x' };
    expect(validateProfile(state, 'mm')).toEqual({});
    const on = { ...state, tabs: true, ramp: true };
    const errors = validateProfile(on, 'mm');
    expect(errors.tabCount).toBeDefined();
    expect(errors.rampDistance).toBeDefined();
    expect(profileParams(on, 'mm')).toBeNull();
  });

  it('loads a calculated toolpath back into the form unchanged', () => {
    const params = profileParams(
      {
        ...initialProfileState('inch'),
        side: 'inside',
        tabs: true,
        ramp: true,
        leadKind: 'straight',
        leadAngle: '30',
        leadOvercut: '0.125',
        leadOut: false,
        lastPass: true,
        reverseLastPass: true,
        tabTriangular: true,
        tabPlacement: 'distance',
        rampKind: 'spiral',
        rampBy: 'angle',
        rampOnLeadIn: true,
        orders: ['grid', 'leftToRight'],
        startAt: 'boundingBox',
        startAnchor: 'top',
        sharpCorners: true,
        sharpInternalCorners: true,
        passes: {
          byCount: true,
          passCount: '3',
          useLastPass: true,
          lastPassThickness: '0.02',
          exact: true,
          list: null,
        },
      },
      'inch',
    );
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(profileParams(profileStateFrom(params, 'Cut out', 'inch'), 'inch')).toEqual(params);
    }
  });

  it('maps the side to open vectors and sends the pass options', () => {
    const state = {
      ...initialProfileState('mm'),
      side: 'inside' as const,
      passes: {
        byCount: true,
        passCount: '4',
        useLastPass: true,
        lastPassThickness: '0.5',
        exact: false,
        list: null,
      },
    };
    const params = profileParams(state, 'mm');
    expect(params?.openSide).toBe('left');
    expect(params?.passCount).toBe(4);
    expect(params?.lastPassThickness).toBe(0.5);
    expect(profileParams({ ...state, side: 'on' }, 'mm')?.openSide).toBe('on');
    const bad = { ...state, passes: { ...state.passes, passCount: '0' } };
    expect(validateProfile(bad, 'mm').passCount).toBeDefined();
  });

  it('checks the tab spacing or count and the ramp angle or distance in use', () => {
    const state = {
      ...initialProfileState('mm'),
      tabs: true,
      tabPlacement: 'distance' as const,
      tabCount: '0',
      tabSpacing: '-1',
      ramp: true,
      rampBy: 'angle' as const,
      rampDistance: 'x',
      rampAngle: '95',
    };
    expect(Object.keys(validateProfile(state, 'mm')).sort()).toEqual(['rampAngle', 'tabSpacing']);
  });

  it('opens a toolpath saved before the new options with their defaults', () => {
    const old = {
      startDepth: 0,
      cutDepth: 6,
      passDepth: null,
      side: 'outside' as const,
      direction: 'climb' as const,
      allowance: 0,
      tabs: { enabled: true, count: 3, length: 8, thickness: 2 },
      ramp: { enabled: false, distance: 20 },
      lead: { kind: 'none' as const, length: 5, angleDeg: 45, radius: 5, overcut: 0 },
    };
    const state = profileStateFrom(old, 'Old', 'mm');
    expect(state.tabPlacement).toBe('count');
    expect(state.leadOut).toBe(true);
    expect(state.lastPass).toBe(false);
    expect(state.passes.byCount).toBe(false);
    expect(state.orders).toEqual([]);
    expect(profileStateFrom({ ...old, order: 'grid' }, 'Old', 'mm').orders).toEqual(['grid']);
  });

  it('drops the advanced options while they are hidden, and shows them for a toolpath that uses one', () => {
    const state = {
      ...initialProfileState('mm'),
      tabs: true,
      lastPass: true,
      ramp: true,
      leadKind: 'arc' as const,
      leadArcAngle: '120',
      orders: ['grid' as const],
      startAt: 'boundingBox' as const,
      sharpCorners: true,
    };
    expect(validateProfile(state, 'mm').leadArcAngle).toBeDefined();
    const hidden = { ...state, advanced: false };
    expect(validateProfile(hidden, 'mm').leadArcAngle).toBeUndefined();
    const params = profileParams(hidden, 'mm');
    expect(params?.tabs.enabled).toBe(false);
    expect(params?.lastPass?.enabled).toBe(false);
    expect(params?.ramp.enabled).toBe(false);
    expect(params?.lead.kind).toBe('none');
    expect(params?.orders).toEqual([]);
    expect(params?.startAt).toBe('optimized');
    expect(params?.sharpCorners).toBe(false);
    if (params !== null) {
      expect(profileStateFrom(params, 'P', 'mm', false).advanced).toBe(false);
      const tabbed = profileParams({ ...state, leadArcAngle: '45' }, 'mm');
      expect(tabbed).not.toBeNull();
      if (tabbed !== null) {
        expect(profileStateFrom(tabbed, 'P', 'mm', false).advanced).toBe(true);
      }
    }
  });

  it('asks a zig zag by angle for its distance too, a smooth ramp by angle not', () => {
    const smooth = { ...initialProfileState('mm'), ramp: true, rampBy: 'angle' as const };
    expect(rampFields(smooth)).toEqual({ by: true, distance: false, angle: true });
    const zigZag = { ...smooth, rampKind: 'zigZag' as const, rampDistance: '12' };
    expect(rampFields(zigZag)).toEqual({ by: true, distance: true, angle: true });
    expect(profileParams(zigZag, 'mm')?.ramp).toMatchObject({ distance: 12, angle: 20 });
    expect(rampFields({ ...zigZag, rampKind: 'spiral' }).distance).toBe(false);
    const onLead = { ...zigZag, leadKind: 'straight' as const, rampOnLeadIn: true };
    expect(rampFields(onLead)).toEqual({ by: false, distance: false, angle: false });
  });

  it('gives a circular lead its angle and the length they make', () => {
    const state = { ...initialProfileState('mm'), leadKind: 'arc' as const, leadArcAngle: '30' };
    expect(profileParams(state, 'mm')?.lead.arcAngleDeg).toBe(30);
    expect(leadArcLength(state, 'mm')).toBeCloseTo((5 * Math.PI) / 6);
    expect(validateProfile({ ...state, leadArcAngle: '0' }, 'mm').leadArcAngle).toBeDefined();
  });

  it('ticks Order options in the page order and starts Closest on Bounding Box at the chosen point', () => {
    const base = initialProfileState('mm');
    const ticked = { ...base, orders: toggleOrder(base, 'grid', true) };
    const both = { ...ticked, orders: toggleOrder(ticked, 'selectionOrder', true) };
    expect(profileParams(both, 'mm')?.orders).toEqual(['selectionOrder', 'grid']);
    expect(toggleOrder(both, 'grid', false)).toEqual(['selectionOrder']);
    const box = { ...base, startAt: 'boundingBox' as const, startAnchor: 'right' as const };
    expect(profileParams(box, 'mm')?.startAnchor).toBe('middleRight');
  });

  it('starts from the defaults: two tabs, a 20 degree ramp, no last pass allowance', () => {
    const state = initialProfileState('mm');
    expect(state.tabCount).toBe('2');
    expect(state.rampAngle).toBe('20');
    expect(state.lastPassAllowance).toBe('0');
  });

  it('offers a spiral ramp only without leads', () => {
    const spiral = { ...initialProfileState('mm'), ramp: true, rampKind: 'spiral' as const };
    expect(spiralAllowed(spiral)).toBe(true);
    expect(profileParams(spiral, 'mm')?.ramp.kind).toBe('spiral');
    const led = { ...spiral, leadKind: 'straight' as const };
    expect(spiralAllowed(led)).toBe(false);
    expect(profileParams(led, 'mm')?.ramp.kind).toBe('smooth');
  });

  it('keeps the tabs placed in Edit Tabs and how Add Tabs places them', () => {
    const state = {
      ...initialProfileState('mm'),
      tabs: true,
      tabRule: 'firstAtStart' as const,
      tabPositions: [{ x: 5, y: 0 }],
    };
    const params = profileParams(state, 'mm');
    expect(params?.tabs.placement).toBe('firstAtStart');
    expect(params?.tabPositions).toEqual([{ x: 5, y: 0 }]);
    if (params !== null) {
      const back = profileStateFrom(params, 'P', 'mm');
      expect(back.tabRule).toBe('firstAtStart');
      expect(back.tabPositions).toEqual([{ x: 5, y: 0 }]);
      expect(tabSpec(back, 'mm')).toEqual(params.tabs);
    }
  });

  it('sends back the anchors that keep placed tabs on their vectors', () => {
    const anchors = [{ id: 7, loopIndex: 0, along: 0.25, at: { x: 5, y: 0 } }, null];
    const loaded = profileStateFrom(
      {
        ...profileParams(initialProfileState('mm'), 'mm'),
        tabPositions: [
          { x: 5, y: 0 },
          { x: 9, y: 9 },
        ],
        tabAnchors: anchors,
      } as ProfileParams,
      'P',
      'mm',
    );
    expect(profileParams(loaded, 'mm')?.tabAnchors).toEqual(anchors);
  });
  it('sends the allowances, tabs and ramp typed in inches as millimeters', () => {
    const state = {
      ...initialProfileState('inch'),
      allowance: '0.01',
      lastPass: true,
      lastPassAllowance: '0.005',
      tabs: true,
      tabLength: '0.5',
      tabThickness: '0.125',
      ramp: true,
      rampDistance: '1',
    };
    const params = profileParams(state, 'inch');
    expect(params?.allowance).toBeCloseTo(0.254, 9);
    expect(params?.lastPass?.allowance).toBeCloseTo(0.127, 9);
    expect(params?.tabs.length).toBeCloseTo(12.7, 9);
    expect(params?.tabs.thickness).toBeCloseTo(3.175, 9);
    expect(params?.ramp.distance).toBeCloseTo(25.4, 9);
  });

  it('opens a new form as the last profile was calculated, with a fresh name', () => {
    expect(newProfileState(null, 'mm', 12)).toEqual(initialProfileState('mm', 12));
    const typed = {
      ...initialProfileState('mm', 18),
      name: 'Profile 1',
      depths: { ...initialProfileState('mm', 18).depths, cutDepth: '12.2' },
      tabs: true,
      tabThickness: '3',
      tabPositions: [{ x: 1, y: 2 }],
    };
    const again = newProfileState({ state: typed, units: 'mm' }, 'mm', 18);
    expect(profileParams(again, 'mm')?.cutDepth).toBeCloseTo(12.2, 9);
    expect(again.tabs).toBe(true);
    expect(again.name).toBe('');
    expect(again.tabPositions).toBeNull();
    // Switched to inches since: the same depth, read in inches.
    const inch = newProfileState({ state: typed, units: 'mm' }, 'inch', 18);
    expect(profileParams(inch, 'inch')?.cutDepth).toBeCloseTo(12.2, 3);
  });
});
