import { describe, expect, it } from 'vitest';

import { VIEW_PRESETS } from '../View3D/camera3d';
import { projectCube } from './model';

describe('orientation cube', () => {
  it('shows only the top face, as a square, looking straight down', () => {
    const faces = projectCube(VIEW_PRESETS.top.theta, VIEW_PRESETS.top.phi, 10);
    expect(faces.map((f) => f.face)).toEqual(['top']);
    const top = faces[0];
    expect(top?.facing).toBeCloseTo(1, 9);
    // Front edge (scene +z) at the bottom of the screen, y down.
    expect(top?.points).toBe('-10,10 10,10 10,-10 -10,-10');
    expect(top?.centre).toEqual({ x: 0, y: 0 });
  });

  it('shows front, right and top from the iso view, nearest last', () => {
    const faces = projectCube(VIEW_PRESETS.iso.theta, VIEW_PRESETS.iso.phi, 10);
    expect(faces.map((f) => f.face).sort()).toEqual(['front', 'right', 'top']);
    const last = faces[faces.length - 1];
    expect(last?.facing).toBe(Math.max(...faces.map((f) => f.facing)));
    const top = faces.find((f) => f.face === 'top');
    // The top face sits above the middle of the cube on screen.
    expect(top?.centre.y).toBeLessThan(0);
  });

  it('shows the left face on the left from the front view turned a little left', () => {
    const faces = projectCube(-0.4, Math.PI / 2, 10);
    const left = faces.find((f) => f.face === 'left');
    const front = faces.find((f) => f.face === 'front');
    expect(left).toBeDefined();
    expect(front).toBeDefined();
    expect(left?.centre.x ?? 0).toBeLessThan(front?.centre.x ?? 0);
    expect(faces.some((f) => f.face === 'right')).toBe(false);
  });
});
