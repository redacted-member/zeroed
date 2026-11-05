import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB, type JobSetup } from '../bridge/types';
import { applyScale, designScale, isIdentityScale } from './designScale';

function job(width: number, height: number, xyOrigin: JobSetup['xyOrigin']): JobSetup {
  return { ...DEFAULT_JOB, width, height, xyOrigin };
}

describe('design scale', () => {
  it('scales about a bottom left datum', () => {
    const scale = designScale(job(100, 50, 'bottomLeft'), job(200, 150, 'bottomLeft'));
    expect(scale).not.toBeNull();
    if (scale === null) {
      return;
    }
    expect(scale.scaleX).toBe(2);
    expect(scale.scaleY).toBe(3);
    expect(applyScale(scale, { x: 25, y: 10 })).toEqual({ x: 50, y: 30 });
    expect(applyScale(scale, { x: 0, y: 0 })).toEqual({ x: 0, y: 0 });
  });

  it('keeps a centre datum in place', () => {
    const scale = designScale(job(100, 50, 'centre'), job(200, 100, 'centre'));
    expect(scale).not.toBeNull();
    if (scale === null) {
      return;
    }
    expect(applyScale(scale, { x: 0, y: 0 })).toEqual({ x: 0, y: 0 });
    expect(applyScale(scale, { x: 10, y: -5 })).toEqual({ x: 20, y: -10 });
  });

  it('keeps fractions of the material for a top right datum', () => {
    const scale = designScale(job(100, 50, 'topRight'), job(50, 100, 'topRight'));
    expect(scale).not.toBeNull();
    if (scale === null) {
      return;
    }
    // The old lower left corner (-100, -50) goes to the new one (-50, -100).
    expect(applyScale(scale, { x: -100, y: -50 })).toEqual({ x: -50, y: -100 });
    expect(applyScale(scale, { x: 0, y: 0 })).toEqual({ x: 0, y: 0 });
  });

  it('knows when nothing moves', () => {
    const same = designScale(job(100, 50, 'bottomLeft'), {
      ...job(100, 50, 'bottomLeft'),
      thickness: 6,
    });
    expect(same !== null && isIdentityScale(same)).toBe(true);
    const moved = designScale(job(100, 50, 'bottomLeft'), job(100, 50, 'centre'));
    expect(moved !== null && isIdentityScale(moved)).toBe(false);
  });

  it('refuses a job with no size', () => {
    expect(designScale(job(0, 50, 'bottomLeft'), job(100, 50, 'bottomLeft'))).toBeNull();
  });
});
