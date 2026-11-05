import { describe, expect, it } from 'vitest';

import { formForToolpath } from './editForm';

describe('toolpath edit form', () => {
  it('opens each laser strategy in its own form', () => {
    expect(formForToolpath({ kind: 'laser', strategy: 'laserCutFill' })).toBe(
      'laserCutFillToolpath',
    );
    expect(formForToolpath({ kind: 'laser', strategy: 'laserPicture' })).toBe(
      'laserPictureToolpath',
    );
    expect(formForToolpath({ kind: 'laser', strategy: 'laserSketch' })).toBe('laserSketchToolpath');
  });

  it('opens Sketch Carving apart from V-Groove, and gadgets in their gadget', () => {
    expect(formForToolpath({ kind: 'vGroove', strategy: 'sketchCarve' })).toBe('sketchToolpath');
    expect(formForToolpath({ kind: 'vGroove', strategy: 'vgroove' })).toBe('vgrooveToolpath');
    expect(formForToolpath({ kind: 'profile', gadget: 'Keyhole' })).toBe('gadget');
    expect(formForToolpath({ kind: 'laser' })).toBeNull();
    expect(formForToolpath({ kind: 'drill', strategy: 'drillBank' })).toBe('drillBankToolpath');
    expect(formForToolpath({ kind: 'drill', strategy: 'drill' })).toBe('drillToolpath');
  });
});
