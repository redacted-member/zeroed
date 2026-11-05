import { describe, expect, it } from 'vitest';
import { copyRequest } from './copyRequest';

describe('copying the selection by a matrix', () => {
  it('takes every selected entity, whatever its kind, with the selected components', () => {
    // 1 a path, 2 text, 5 a group: the kernel copies each whole.
    expect(copyRequest([1, 2, 5], [7])).toEqual({ vectors: [1, 2, 5], components: [7] });
  });

  it('copies components on their own', () => {
    expect(copyRequest([], [7, 8])).toEqual({ vectors: [], components: [7, 8] });
  });

  it('copies only the ids a form names, without the components', () => {
    expect(copyRequest([1, 2, 5], [7], [2])).toEqual({ vectors: [2], components: [] });
  });

  it('takes each object once and nothing when nothing is selected', () => {
    expect(copyRequest([3, 3], [4, 4])).toEqual({ vectors: [3], components: [4] });
    expect(copyRequest([], [])).toBeNull();
    expect(copyRequest([1], [7], [])).toBeNull();
  });
});
