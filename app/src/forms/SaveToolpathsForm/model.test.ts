import { describe, expect, it } from 'vitest';

import type { PostSummary, ToolpathSummary } from '../../bridge/cam';
import type { MachinePrefs } from '../../bridge/prefs';
import {
  canOutputDirect,
  initialOutput,
  capabilityProblem,
  firstPost,
  postsForMachine,
  rotaryNote,
  rotaryWarning,
  savedToolpaths,
  suggestedName,
  toolChangeProblem,
  trialCut,
  trialDefault,
  trialName,
} from './model';

function toolpath(id: number, toolName: string, toolNumber: number, visible = true) {
  return {
    id,
    name: `Toolpath ${id}`,
    kind: 'profile',
    visible,
    stale: false,
    toolName,
    toolNumber,
    estimatedSeconds: 0,
    moves: 3,
  } satisfies ToolpathSummary;
}

function post(id: string, toolChange: boolean): PostSummary {
  return { id, name: id, fileExtension: 'nc', units: 'mm', arcs: true, toolChange };
}

const machines: MachinePrefs = {
  active: 'Router',
  list: [
    {
      name: 'Router',
      units: 'mm',
      bedX: 600,
      bedY: 400,
      bedZ: 100,
      posts: ['b', 'c'],
      defaultPost: 'c',
    },
    { name: 'Bare', units: 'mm', bedX: 600, bedY: 400, bedZ: 100, posts: [] },
  ],
};

describe('save toolpaths form', () => {
  const list = [toolpath(1, 'End Mill', 1), toolpath(2, 'V Bit', 2, false), toolpath(3, 'Ball', 3)];

  it('saves the selected toolpath or the visible ones', () => {
    expect(savedToolpaths('selected', list, 2).map((t) => t.id)).toEqual([2]);
    expect(savedToolpaths('selected', list, null)).toEqual([]);
    expect(savedToolpaths('oneFile', list, 2).map((t) => t.id)).toEqual([1, 3]);
    expect(savedToolpaths('multipleFiles', list, null).map((t) => t.id)).toEqual([1, 3]);
  });

  it('offers the posts of the machine and picks its default', () => {
    const posts = [post('a', true), post('b', true), post('c', false)];
    expect(postsForMachine(posts, machines, 'Router').map((p) => p.id)).toEqual(['b', 'c']);
    expect(postsForMachine(posts, machines, 'Bare')).toHaveLength(3);
    expect(firstPost(posts, machines, 'Router')).toBe('c');
    expect(firstPost(posts, machines, 'Bare')).toBe('a');
  });

  it('lists the posts by name and starts a bare machine on the generic post of the job unit', () => {
    const posts = [
      { ...post('mach_mm', true), name: 'Mach (mm)' },
      { ...post('generic_gcode_mm', true), name: 'Generic G code (mm)' },
      { ...post('generic_gcode_inch', true), name: 'Generic G code (inch)' },
      { ...post('centroid_mm', true), name: 'Centroid (mm)' },
    ];
    expect(postsForMachine(posts, machines, 'Bare').map((p) => p.name)).toEqual([
      'Centroid (mm)',
      'Generic G code (inch)',
      'Generic G code (mm)',
      'Mach (mm)',
    ]);
    expect(firstPost(posts, machines, 'Bare', 'mm')).toBe('generic_gcode_mm');
    expect(firstPost(posts, machines, 'Bare', 'inch')).toBe('generic_gcode_inch');
    // A machine with a default keeps it.
    const withDefault = [...posts, post('c', true)];
    expect(firstPost(withDefault, machines, 'Router', 'inch')).toBe('c');
  });

  it('suggests a laser post for laser toolpaths', () => {
    const laser = (id: string, units: 'mm' | 'inch' = 'mm') => ({
      ...post(id, false),
      jet: true,
      units,
    });
    const posts = [
      { ...post('generic_gcode_mm', true), name: 'Generic G code (mm)' },
      { ...laser('laser_inch', 'inch'), name: 'A laser (inch)' },
      { ...laser('laser_grbl'), name: 'GRBL laser (mm)' },
    ];
    // The Default machine offers every post: its milling pick stays for milling.
    expect(firstPost(posts, machines, 'Bare', 'mm')).toBe('generic_gcode_mm');
    expect(firstPost(posts, machines, 'Bare', 'mm', true)).toBe('laser_grbl');
    expect(firstPost(posts, machines, 'Bare', 'inch', true)).toBe('laser_inch');
    // A machine whose default drives a laser keeps it; one with no laser
    // post keeps its default and the form says why it cannot save.
    const own = [post('b', true), laser('c')];
    expect(firstPost(own, machines, 'Router', 'mm', true)).toBe('c');
    const milling = [post('b', true), post('c', true)];
    expect(firstPost(milling, machines, 'Router', 'mm', true)).toBe('c');
  });

  it('needs a tool change post and distinct numbers for several tools in one file', () => {
    const saved = [list[0], list[2]].filter((t) => t !== undefined);
    expect(toolChangeProblem('oneFile', saved, post('p', false))).toMatch(/cannot change tools/);
    expect(toolChangeProblem('oneFile', saved, post('p', true))).toBeNull();
    expect(toolChangeProblem('multipleFiles', saved, post('p', false))).toBeNull();
    const clash = [toolpath(1, 'End Mill', 1), toolpath(4, 'Ball', 1)];
    expect(toolChangeProblem('oneFile', clash, post('p', true))).toMatch(/tool number 1/);
    const same = [toolpath(1, 'End Mill', 1), toolpath(5, 'End Mill', 1)];
    expect(toolChangeProblem('oneFile', same, post('p', false))).toBeNull();
  });

  it('suggests a file name', () => {
    expect(suggestedName('selected', [toolpath(1, 'E', 1)], 'nc')).toBe('Toolpath 1.nc');
    expect(suggestedName('multipleFiles', list, 'gcode')).toBe('Toolpaths.gcode');
  });

  it('needs a laser post for laser toolpaths and warns a flat post of a rotary job', () => {
    const laserPath = { kind: 'laser', toolNumber: 1, toolName: 'Laser' } as ToolpathSummary;
    const millPath = { kind: 'profile', toolNumber: 1, toolName: 'Mill' } as ToolpathSummary;
    const grbl = { id: 'grbl_mm', name: 'GRBL', fileExtension: 'gcode', units: 'mm', arcs: true };
    const laser = { ...grbl, id: 'laser', name: 'Laser', jet: true } as PostSummary;
    const rotary = {
      ...grbl,
      id: 'rot',
      name: 'Rotary',
      rotary: true,
      rotaryWrapY: 'A',
    } as PostSummary;
    expect(capabilityProblem([laserPath], grbl as PostSummary)).toContain('GRBL');
    expect(capabilityProblem([laserPath], laser)).toBeNull();
    expect(capabilityProblem([millPath], grbl as PostSummary)).toBeNull();
    expect(capabilityProblem([laserPath], null)).toBeNull();
    const bankPath = { ...millPath, strategy: 'drillBank' };
    expect(capabilityProblem([bankPath], grbl as PostSummary)).toContain('drill bank');
    expect(capabilityProblem([bankPath], { ...(grbl as PostSummary), drillBank: true })).toBeNull();
    expect(rotaryWarning(grbl as PostSummary, 'x')).toContain('GRBL');
    expect(rotaryWarning(rotary, 'x')).toBeNull();
    expect(rotaryWarning(grbl as PostSummary, null)).toBeNull();
    // A post that wraps only one way writes the other blank flat: it warns.
    expect(rotaryWarning(rotary, 'y')).toContain('along Y');
    const wiring = { letter: 'A', units: 'degrees', reversed: false } as const;
    expect(rotaryNote(rotary, 'x', wiring)).toContain('turns the A axis in degrees.');
    expect(rotaryNote(rotary, 'y', wiring)).toBeNull();
    expect(rotaryNote(rotary, null, wiring)).toBeNull();
    // The note says what the machine is set to, not what the post was written for.
    expect(rotaryNote(rotary, 'x', { letter: 'B', units: 'linear', reversed: true })).toContain(
      'turns the B axis in linear units, reversed.',
    );
  });

  it('picks a post that wraps a rotary job, from the same controller first', () => {
    const summary = (id: string, units: 'mm' | 'inch', y: string | null, x: string | null) =>
      ({
        id,
        name: id,
        fileExtension: 'nc',
        units,
        arcs: true,
        rotaryWrapY: y,
        rotaryWrapX: x,
        rotary: y !== null || x !== null,
      }) as PostSummary;
    const posts = [
      summary('generic_gcode_mm', 'mm', null, null),
      summary('grbl_rotary_a_mm', 'mm', 'A', null),
      summary('linuxcnc_mm', 'mm', null, null),
      summary('linuxcnc_rotary_inch', 'inch', 'A', 'A'),
      summary('linuxcnc_rotary_mm', 'mm', 'A', 'A'),
      summary('mach3_rotary_mm', 'mm', 'A', 'B'),
    ];
    const machines: MachinePrefs = {
      active: 'Router',
      list: [
        {
          name: 'Router',
          units: 'mm',
          travelX: 1200,
          travelY: 2400,
          travelZ: 150,
          posts: [],
          defaultPost: 'linuxcnc_mm',
        },
      ],
    } as unknown as MachinePrefs;
    // The flat job keeps the machine's own post.
    expect(firstPost(posts, machines, 'Router', 'mm')).toBe('linuxcnc_mm');
    expect(firstPost(posts, machines, 'Router', 'mm', false, 'x')).toBe('linuxcnc_rotary_mm');
    expect(firstPost(posts, machines, 'Router', 'inch', false, 'y')).toBe('linuxcnc_rotary_inch');
  });

  it('adds the side to the suggested name when asked', () => {
    expect(suggestedName('selected', [toolpath(1, 'Profile 1', 1)], 'nc', 'Top')).toBe(
      'Toolpath 1 (Top).nc',
    );
  });
});

describe('save toolpaths remembered and direct output', () => {
  it('opens on the last output choice unless opened for one toolpath', () => {
    expect(initialOutput(null, 'multipleFiles')).toBe('multipleFiles');
    expect(initialOutput([4], 'oneFile')).toBe('selected');
    expect(initialOutput([4, 5], 'oneFile')).toBe('oneFile');
  });

  it('offers direct output only for a post with a driver on a host that can start it', () => {
    const post = {
      id: 'p',
      name: 'P',
      fileExtension: 'nc',
      units: 'mm' as const,
      arcs: true,
      directOutput: 'Controller',
    };
    const desktop = {
      directOutputPath: (name: string) => Promise.resolve(name),
      sendToDriver: () => Promise.resolve(),
    };
    expect(canOutputDirect(post, desktop)).toBe('Controller');
    expect(canOutputDirect(post, {})).toBeNull();
    expect(canOutputDirect({ ...post, directOutput: ' ' }, desktop)).toBeNull();
    expect(canOutputDirect(null, desktop)).toBeNull();
  });
});

describe('trial runs', () => {
  it('start an air cut 25 mm or 1 in up and a scrap cut 0.5 mm or 0.02 in deep', () => {
    expect(trialDefault('air', 'mm')).toBe('25');
    expect(trialDefault('air', 'inch')).toBe('1');
    expect(trialDefault('scrap', 'mm')).toBe('0.5');
    expect(trialDefault('scrap', 'inch')).toBe('0.02');
    expect(trialDefault('none', 'mm')).toBe('');
  });

  it('send the height and depth in millimetres', () => {
    expect(trialCut('air', '25', 'mm')).toEqual({
      trial: { kind: 'air', height: 25 },
      error: null,
    });
    const inch = trialCut('air', '1', 'inch').trial;
    expect(inch?.kind === 'air' ? inch.height : null).toBeCloseTo(25.4, 9);
    const scrap = trialCut('scrap', '0.02', 'inch').trial;
    expect(scrap?.kind === 'scrap' ? scrap.depth : null).toBeCloseTo(0.508, 9);
    expect(trialCut('scrap', '0.5', 'mm').trial).toEqual({ kind: 'scrap', depth: 0.5 });
    expect(trialCut('none', 'x', 'mm')).toEqual({ trial: null, error: null });
  });

  it('refuse a height or depth that is not above zero', () => {
    expect(trialCut('air', '0', 'mm').error).not.toBeNull();
    expect(trialCut('scrap', '-0.5', 'mm').error).not.toBeNull();
    expect(trialCut('scrap', 'deep', 'mm').error).not.toBeNull();
    expect(trialCut('scrap', '0.01', 'mm').error).toBeNull();
  });

  it('label the file name before its extension', () => {
    expect(trialName('Pocket 1.ngc', 'air')).toBe('Pocket 1_AIR_CUT.ngc');
    expect(trialName('Pocket 1.tap', 'scrap')).toBe('Pocket 1_SCRAP_CUT.tap');
    expect(trialName('Toolpaths', 'air')).toBe('Toolpaths_AIR_CUT');
    expect(trialName('Pocket 1.ngc', 'none')).toBe('Pocket 1.ngc');
  });
});
