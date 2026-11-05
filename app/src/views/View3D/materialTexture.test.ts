import { describe, expect, it } from 'vitest';

import { texturePathFor } from './materialTexture';

describe('material texture', () => {
  it('finds the picture of the texture a job asks for', () => {
    const textures = [{ id: 2, name: 'Oak burl', category: 'Wood', path: '/t/burl.png' }];
    expect(texturePathFor({ material: 'oak', colour: '#ffffff', texture: 2 }, textures)).toBe(
      '/t/burl.png',
    );
    expect(texturePathFor({ material: 'oak', colour: '#ffffff' }, textures)).toBeNull();
    expect(texturePathFor({ material: 'oak', colour: '#ffffff', texture: 9 }, textures)).toBeNull();
    expect(texturePathFor(undefined, textures)).toBeNull();
  });
});
