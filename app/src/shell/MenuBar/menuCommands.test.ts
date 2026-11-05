import { describe, expect, it } from 'vitest';

import { importKindOf, incrementedPath } from './menuCommands';

describe('incrementedPath', () => {
  it('adds _1, then counts up', () => {
    expect(incrementedPath('/jobs/sign.carve')).toBe('/jobs/sign_1.carve');
    expect(incrementedPath('/jobs/sign_1.carve')).toBe('/jobs/sign_2.carve');
    expect(incrementedPath('/jobs/sign_09.carve')).toBe('/jobs/sign_10.carve');
    expect(incrementedPath('C:\\jobs\\a.b\\sign')).toBe('C:\\jobs\\a.b\\sign_1');
  });
});

describe('importKindOf', () => {
  it('sends pictures to the bitmap import and drawings to the vector import', () => {
    expect(importKindOf('/a/logo.PNG')).toBe('bitmap');
    expect(importKindOf('/a/photo.jpeg')).toBe('bitmap');
    expect(importKindOf('/a/plan.dxf')).toBe('vectors');
    expect(importKindOf('/a/model.stl')).toBeNull();
  });
});
