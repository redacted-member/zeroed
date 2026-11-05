import { describe, expect, it } from 'vitest';

import { chooseTab, tabPressed } from './viewTabs';

describe('view tabs', () => {
  it('shows the Assembly View tab selected while its form is open, and not the other two', () => {
    expect(tabPressed('assembly', '3d', 'assemblyView')).toBe(true);
    expect(tabPressed('3d', '3d', 'assemblyView')).toBe(false);
    expect(tabPressed('2d', 'split', 'assemblyView')).toBe(false);
    expect(tabPressed('assembly', '3d', null)).toBe(false);
    expect(tabPressed('3d', '3d', 'jobSetup')).toBe(true);
    expect(tabPressed('2d', 'split', null)).toBe(true);
  });

  it('opens the Assembly View over the 3D view, and 2D or 3D View leaves it', () => {
    expect(chooseTab('assembly', null)).toEqual({ viewMode: '3d', form: 'open' });
    expect(chooseTab('assembly', 'jobSetup')).toEqual({ viewMode: '3d', form: 'open' });
    expect(chooseTab('assembly', 'assemblyView')).toEqual({ viewMode: '3d', form: 'keep' });
    expect(chooseTab('2d', 'assemblyView')).toEqual({ viewMode: '2d', form: 'close' });
    expect(chooseTab('3d', 'assemblyView')).toEqual({ viewMode: '3d', form: 'close' });
    expect(chooseTab('3d', 'jobSetup')).toEqual({ viewMode: '3d', form: 'keep' });
  });

  it('keeps the Assembly View tab for the Furniture Designer, which shows over it', () => {
    expect(tabPressed('assembly', '3d', 'furnitureDesigner')).toBe(true);
    expect(tabPressed('3d', '3d', 'furnitureDesigner')).toBe(false);
    expect(chooseTab('assembly', 'furnitureDesigner')).toEqual({ viewMode: '3d', form: 'keep' });
    expect(chooseTab('2d', 'furnitureDesigner')).toEqual({ viewMode: '2d', form: 'close' });
    expect(chooseTab('assembly', 'jointCreator')).toEqual({ viewMode: '3d', form: 'open' });
  });
});
