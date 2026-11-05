import { describe, expect, it } from 'vitest';

import { strings } from './formStrings';
import { strings as startStrings } from './strings';

describe('formStrings', () => {
  it('keeps the start strings of a group and adds its deferred ones', () => {
    expect(strings.toolDatabase.title).toBe(startStrings.toolDatabase.title);
    expect(strings.toolDatabase.select).toBe('Select');
    expect(strings.toolpathShell.list.heading).toBe(startStrings.toolpathShell.list.heading);
    expect(strings.toolpathShell.list.missingCreate).toBe('Auto-create Layers');
  });

  it('shows the deferred toolpath list strings under the toolpaths area too', () => {
    expect(strings.toolpaths.list.missingCreate).toBe('Auto-create Layers');
    expect(strings.toolpaths.imported.title).toBe(startStrings.toolpathShell.imported.title);
  });

  it('leaves the areas that only forms use whole', () => {
    expect(typeof strings.form.buttonTips).toBe('object');
    expect(strings.usageInfo.label).toBe('3D view controls');
  });
});
