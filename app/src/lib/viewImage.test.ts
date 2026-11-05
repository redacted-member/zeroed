import { describe, expect, it } from 'vitest';

import { VIEW_IMAGE_FILTERS, viewImageName, viewImagePath } from './viewImage';

describe('save 2D view as image', () => {
  it('names the image after the job', () => {
    expect(viewImageName('/jobs/Sign board.carve')).toBe('Sign board 2D View.png');
    expect(viewImageName(null)).toBe('Untitled 2D View.png');
  });

  it('keeps a format the kernel writes and adds PNG otherwise', () => {
    expect(viewImagePath('view.jpg')).toBe('view.jpg');
    expect(viewImagePath('view.JPEG')).toBe('view.JPEG');
    expect(viewImagePath('view.bmp')).toBe('view.bmp');
    expect(viewImagePath('view')).toBe('view.png');
    expect(viewImagePath('view.gif')).toBe('view.gif');
    expect(viewImagePath('view.tif')).toBe('view.tif.png');
  });

  it('offers PNG first', () => {
    expect(VIEW_IMAGE_FILTERS[0]?.extensions).toEqual(['png']);
  });
});
