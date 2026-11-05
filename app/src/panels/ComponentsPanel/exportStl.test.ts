import { describe, expect, it } from 'vitest';

import { meshFormatOf } from '../../forms/ExportStlForm/model';
import { meshFilters, stlDefaultName, stlExportIds } from './exportStl';

describe('export model', () => {
  it('exports the selected component, or the whole model', () => {
    expect(stlExportIds(4)).toEqual([4]);
    expect(stlExportIds(null)).toBeUndefined();
  });

  it('offers a safe file name', () => {
    expect(stlDefaultName('Lion head')).toBe('Lion head.stl');
    expect(stlDefaultName('a/b:c')).toBe('a_b_c.stl');
    expect(stlDefaultName(null)).toBe('Untitled.stl');
    expect(stlDefaultName('Lion', 'png')).toBe('Lion.png');
  });

  it('lets the save dialog pick STL, OBJ or POV-Ray, and writes the type chosen', () => {
    expect(meshFilters().map((f) => f.extensions[0])).toEqual(['stl', 'obj', 'pov']);
    expect(meshFormatOf('/tmp/lion.OBJ')).toBe('obj');
    expect(meshFormatOf('/tmp/lion.pov')).toBe('pov');
    expect(meshFormatOf('/tmp/lion')).toBe('stl');
    expect(meshFormatOf('/tmp/v1.2/lion.txt')).toBe('stl');
  });
});
