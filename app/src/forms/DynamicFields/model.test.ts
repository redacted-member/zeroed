import { describe, expect, it } from 'vitest';

import {
  dynamicState,
  fadeChanged,
  fadeProp,
  tiltChanged,
  tiltProp,
  validateDynamic,
  withAnchors,
} from './model';

const a = { x: 1, y: 2 };
const b = { x: 11, y: 2 };

describe('fade and tilt settings', () => {
  it('starts off, and reads a component back', () => {
    const off = dynamicState();
    expect(off).toMatchObject({ fadeOn: false, tiltOn: false, fadePercent: '50', tiltAngle: '5' });
    expect(fadeProp(off)).toBeNull();
    expect(tiltProp(off)).toBeNull();
    const on = dynamicState(
      { from: a, to: b, amount: 0.25 },
      { pivot: a, toward: b, angleDeg: -7.5 },
    );
    expect(on).toMatchObject({ fadeOn: true, fadePercent: '25', tiltOn: true, tiltAngle: '-7.5' });
    expect(fadeProp(on)).toEqual({ from: a, to: b, amount: 0.25 });
    expect(tiltProp(on)).toEqual({ pivot: a, toward: b, angleDeg: -7.5 });
  });

  it('waits for two separate anchors and a good value', () => {
    const on = { ...dynamicState(), fadeOn: true, tiltOn: true };
    expect(fadeProp(on)).toBeUndefined();
    const picked = withAnchors(withAnchors(on, 'fade', [a, b]), 'tilt', [a, a]);
    expect(fadeProp(picked)).toEqual({ from: a, to: b, amount: 0.5 });
    expect(tiltProp(picked)).toBeUndefined();
    const bad = { ...picked, fadePercent: '120', tiltAngle: '90' };
    expect(Object.keys(validateDynamic(bad)).sort()).toEqual(['fadePercent', 'tiltAngle']);
    expect(fadeProp(bad)).toBeUndefined();
    expect(withAnchors(on, 'fade', [a])).toBe(on);
  });

  it('turns a check box on when its anchors are picked', () => {
    const next = withAnchors(dynamicState(), 'tilt', [a, b]);
    expect(next).toMatchObject({ tiltOn: true, tiltPivot: a, tiltToward: b });
  });

  it('tells what changed from the component', () => {
    const fade = { from: a, to: b, amount: 0.5 };
    expect(fadeChanged(fade, { ...fade })).toBe(false);
    expect(fadeChanged(fade, { ...fade, amount: 0.4 })).toBe(true);
    expect(fadeChanged(null, null)).toBe(false);
    expect(fadeChanged(null, fade)).toBe(true);
    const tilt = { pivot: a, toward: b, angleDeg: 5 };
    expect(tiltChanged(tilt, undefined)).toBe(true);
    expect(tiltChanged(tilt, { ...tilt, toward: { x: 11, y: 3 } })).toBe(true);
    expect(tiltChanged(tilt, { ...tilt })).toBe(false);
  });
});
