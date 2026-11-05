import { describe, expect, it } from 'vitest';

import { exportName } from './model';

describe('exportName', () => {
  it('names the file after the job, or Assembly for a new job', () => {
    expect(exportName('/jobs/Shelf unit.carve', 'stl')).toBe('Shelf unit Assembly.stl');
    expect(exportName('C:\\jobs\\Box.carve', '3mf')).toBe('Box Assembly.3mf');
    expect(exportName(null, 'stl')).toBe('Assembly.stl');
  });
});
