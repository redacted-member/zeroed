import { describe, expect, it } from 'vitest';

import type { MachinePrefs } from '../../bridge/prefs';
import {
  activeMachine,
  activeSavePost,
  addMachine,
  addPost,
  deleteMachine,
  draftFrom,
  draftPrefs,
  duplicateMachine,
  editActive,
  machineProblems,
  withAccelUnits,
  removePost,
  toolDbChanges,
  withUnits,
} from './model';

const PREFS: MachinePrefs = {
  active: 'Shop',
  list: [
    {
      name: 'Default',
      units: 'mm',
      bedX: 600,
      bedY: 600,
      bedZ: 100,
      posts: [],
      rapidRate: 3000,
      acceleration: [0, 0, 0],
      rotaryAcceleration: 0,
      rotary: { letter: 'A', units: 'degrees', reversed: false },
      accelerationUnits: 'mmPerSec2',
    },
    {
      name: 'Shop',
      units: 'inch',
      bedX: 1219.2,
      bedY: 609.6,
      bedZ: 101.6,
      posts: ['grbl_inch', 'mach3_inch'],
      defaultPost: 'mach3_inch',
      rapidRate: 2540,
      acceleration: [254, 254, 127],
      rotaryAcceleration: 90,
      rotary: { letter: 'A', units: 'degrees', reversed: false },
      accelerationUnits: 'inchPerSec2',
    },
  ],
};

describe('machine configuration draft', () => {
  it('opens on the active machine with travel in its own units', () => {
    const draft = draftFrom(PREFS);
    const shop = activeMachine(draft);
    expect(shop?.name).toBe('Shop');
    expect(shop?.bedX).toBe('48');
    expect(shop?.defaultPost).toBe('mach3_inch');
  });

  it('round trips to the same preferences', () => {
    expect(draftPrefs(draftFrom(PREFS))).toEqual(PREFS);
  });

  it('adds, duplicates and deletes machines with free names', () => {
    let draft = addMachine(draftFrom(PREFS), 'mm');
    expect(activeMachine(draft)?.name).toBe('Machine');
    draft = addMachine(draft, 'mm');
    expect(activeMachine(draft)?.name).toBe('Machine 2');
    draft = duplicateMachine(draft);
    expect(activeMachine(draft)?.name).toBe('Machine 2 Copy');
    expect(activeMachine(draft)?.original).toBeNull();
    draft = deleteMachine(draft);
    expect(activeMachine(draft)?.name).toBe('Machine 2');
    expect(draft.machines).toHaveLength(4);
  });

  it('converts the travel when the units change', () => {
    const draft = editActive(draftFrom(PREFS), (m) => withUnits(m, 'mm'));
    expect(activeMachine(draft)?.bedX).toBe('1219.2');
    expect(draftPrefs(draft)?.list[1]?.bedX).toBeCloseTo(1219.2);
  });

  it('refuses empty or repeated names and bad travel', () => {
    let draft = editActive(draftFrom(PREFS), (m) => ({ ...m, name: ' default ' }));
    const shop = activeMachine(draft);
    expect(shop && machineProblems(draft, shop).name).toBeDefined();
    expect(draftPrefs(draft)).toBeNull();
    draft = editActive(draft, (m) => ({ ...m, name: 'Big', bedY: '0' }));
    const big = activeMachine(draft);
    expect(big && machineProblems(draft, big)).toEqual({ bedY: expect.any(String) });
  });

  it('keeps the default post one of the associated posts', () => {
    const shop = activeMachine(draftFrom(PREFS));
    if (shop === null) {
      throw new Error('no active machine');
    }
    const removed = removePost(shop, 'mach3_inch');
    expect(removed.defaultPost).toBe('grbl_inch');
    const added = addPost({ ...shop, posts: [], defaultPost: null }, 'grbl_mm');
    expect(added.defaultPost).toBe('grbl_mm');
    expect(addPost(added, 'grbl_mm').posts).toEqual(['grbl_mm']);
  });
});

describe('toolDbChanges', () => {
  it('renames, removes and adds tool database machines to match', () => {
    let draft = draftFrom(PREFS);
    draft = editActive(draft, (m) => ({ ...m, name: 'Big Shop' }));
    draft = { ...draft, active: draft.machines[0]?.key ?? null };
    draft = deleteMachine(draft);
    draft = addMachine(draft, 'mm');
    const changes = toolDbChanges(['Default', 'Shop'], draft, ['Default', 'Shop', 'Other']);
    expect(changes).toEqual({
      remove: ['Default'],
      rename: [{ from: 'Shop', to: 'Big Shop' }],
      add: ['Machine'],
    });
  });

  it('adds saved machines the tool database does not know yet', () => {
    expect(toolDbChanges(['Default', 'Shop'], draftFrom(PREFS), ['Default'])).toEqual({
      remove: [],
      rename: [],
      add: ['Shop'],
    });
  });
});

describe('activeSavePost', () => {
  it('answers the active machine default post, then its first, then null', () => {
    expect(activeSavePost(PREFS)).toBe('mach3_inch');
    expect(activeSavePost({ ...PREFS, active: 'Default' })).toBeNull();
    const first = PREFS.list.map((m) => ({ ...m, defaultPost: undefined }));
    expect(activeSavePost({ active: 'Shop', list: first })).toBe('grbl_inch');
  });

  it('keeps a rapid rate and accelerations for time estimates', () => {
    const draft = draftFrom(PREFS);
    const shop = activeMachine(draft);
    expect(shop?.rapidRate).toBe('100');
    expect(shop?.accelZ).toBe('5');
    const zero = editActive(draft, (m) => ({ ...m, rapidRate: '0' }));
    const bad = activeMachine(zero);
    expect(bad === null ? {} : machineProblems(zero, bad).rapidRate).toBeDefined();
    const negative = editActive(draft, (m) => ({ ...m, accelX: '-1' }));
    const worse = activeMachine(negative);
    expect(worse === null ? {} : machineProblems(negative, worse).accelX).toBeDefined();
    const old = draftFrom({
      active: 'Old',
      list: [{ name: 'Old', units: 'mm', bedX: 600, bedY: 600, bedZ: 100, posts: [] }],
    });
    expect(draftPrefs(old)?.list[0]).toMatchObject({ rapidRate: 3000, acceleration: [0, 0, 0] });
  });

  it('shows accelerations in the chosen unit and keeps them per second squared', () => {
    const draft = draftFrom(PREFS);
    const shop = activeMachine(draft);
    expect(shop?.accelX).toBe('10');
    expect(shop?.rotaryAccel).toBe('90');
    const perMinute = editActive(draft, (m) => withAccelUnits(m, 'mmPerMin2'));
    expect(activeMachine(perMinute)?.accelX).toBe(String(254 * 3600));
    const saved = draftPrefs(perMinute)?.list[1];
    expect(saved?.acceleration).toEqual([254, 254, 127]);
    expect(saved?.accelerationUnits).toBe('mmPerMin2');
    expect(saved?.rotaryAcceleration).toBe(90);
    expect(saved?.rotary).toEqual({ letter: 'A', units: 'degrees', reversed: false });
  });
});

describe('rotary axis wiring', () => {
  const machine = {
    name: 'Router',
    units: 'inch',
    bedX: 1219.2,
    bedY: 2438.4,
    bedZ: 152.4,
    posts: [],
    rotary: { letter: 'B', units: 'linear', reversed: true },
  } as const;

  it('reads and saves each machine its own wiring', () => {
    const draft = draftFrom({ active: 'Router', list: [{ ...machine, posts: [] }] });
    const first = draft.machines[0];
    expect(first?.rotaryLetter).toBe('B');
    expect(first?.rotaryUnits).toBe('linear');
    expect(first?.rotaryReversed).toBe(true);
    expect(draftPrefs(draft)?.list[0]?.rotary).toEqual({
      letter: 'B',
      units: 'linear',
      reversed: true,
    });
  });

  it('reads old preferences with no wiring as A in degrees, the normal way', () => {
    const old = { name: 'Old', units: 'mm', bedX: 600, bedY: 600, bedZ: 100, posts: [] } as const;
    const draft = draftFrom({ active: 'Old', list: [{ ...old, posts: [] }] });
    expect(draftPrefs(draft)?.list[0]?.rotary).toEqual({
      letter: 'A',
      units: 'degrees',
      reversed: false,
    });
  });

  it('starts a new machine on A in degrees and copies wiring with a duplicate', () => {
    const draft = draftFrom({ active: 'Router', list: [{ ...machine, posts: [] }] });
    const added = addMachine(draft, 'inch');
    expect(activeMachine(added)?.rotaryLetter).toBe('A');
    expect(activeMachine(added)?.rotaryUnits).toBe('degrees');
    expect(activeMachine(added)?.rotaryReversed).toBe(false);
    const copy = duplicateMachine(draft);
    expect(activeMachine(copy)?.rotaryLetter).toBe('B');
    expect(activeMachine(copy)?.rotaryReversed).toBe(true);
  });
});
