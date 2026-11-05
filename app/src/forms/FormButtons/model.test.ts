import { describe, expect, it } from 'vitest';

import { strings } from '../../lib/formStrings';
import { toolpathsStrings } from '../../lib/strings/toolpaths';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';
import { buttonLook } from './model';

const tips = strings.form.buttonTips;

describe('form action buttons', () => {
  it('give each button an icon and a tooltip that says what it does', () => {
    expect(buttonLook('Calculate')).toEqual({
      icon: 'formCalculate',
      tip: `Calculate\n${tips.calculate}`,
    });
    expect(buttonLook('Close').icon).toBe('dismiss');
    expect(buttonLook('Cancel').tip).toBe(`Cancel\n${tips.cancel}`);
    expect(buttonLook('Bake Components').tip).toBe(`Bake Components\n${tips.bake}`);
    expect(buttonLook('Running...').icon).toBe('formRun');
    expect(buttonLook('Delete All Tiling Data').icon).toBe('editDelete');
  });

  it('give a button the list does not know the Apply icon and its name as tooltip', () => {
    expect(buttonLook('Weld')).toEqual({ icon: 'formApply', tip: 'Weld' });
  });

  it('say what a button that is off waits for', () => {
    const why = toolpathsStrings.toolpaths.needsSelection;
    expect(buttonLook('Calculate', why).tip).toBe(`Calculate\n${why}`);
  });

  it('draw every icon they name', async () => {
    await loadExtraIcons();
    const labels = [...Object.keys(tips), 'weld'];
    for (const label of labels) {
      const { icon } = buttonLook(label);
      expect(toolIcon(icon), icon).not.toBeNull();
    }
  });
});
