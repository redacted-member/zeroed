import { describe, expect, it } from 'vitest';

import { MATERIALS } from '../../views/View3D/appearance';
import {
  chooseColour,
  chooseMaterial,
  chooseTexture,
  materialChoices,
  usesSolidColour,
} from './model';

describe('material appearance dialog model', () => {
  it('lists Use Solid Color first, then every material once', () => {
    const choices = materialChoices(MATERIALS);
    expect(choices[0]).toBe('solid');
    expect([...choices].sort()).toEqual([...MATERIALS].sort());
  });

  it('offers the color picker only for the solid color', () => {
    const draft = { material: 'oak' as const, colour: '#123456' };
    expect(usesSolidColour(draft)).toBe(false);
    const solid = chooseMaterial(draft, 'solid');
    expect(usesSolidColour(solid)).toBe(true);
    expect(solid.colour).toBe('#123456');
  });

  it('takes only a #rrggbb color', () => {
    const draft = { material: 'solid' as const, colour: '#123456' };
    expect(chooseColour(draft, '#ABCDEF').colour).toBe('#abcdef');
    expect(chooseColour(draft, 'red')).toBe(draft);
  });

  it('picks a texture and drops it for a plain material', () => {
    const draft = { material: 'oak' as const, colour: '#123456' };
    const textured = chooseTexture(draft, 7);
    expect(textured).toMatchObject({ material: 'oak', texture: 7 });
    expect(chooseMaterial(textured, 'walnut').texture).toBe(0);
  });
});
