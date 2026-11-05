import { describe, expect, it } from 'vitest';

import {
  IDLE_STATE,
  commandEnabled,
  gatedCommands,
  toolEnabled,
  type EnableState,
} from './enableState';

const with_ = (patch: Partial<EnableState>): EnableState => ({ ...IDLE_STATE, ...patch });

describe('commandEnabled', () => {
  it('runs commands without a rule in every state', () => {
    for (const command of [
      'form.move',
      'edit.group',
      'align.lastLeft',
      'view.zoomToJob',
    ] as const) {
      expect(commandEnabled(command, IDLE_STATE)).toBe(true);
    }
  });

  it('needs a selection of vectors or components for Cut, Copy and Delete', () => {
    for (const command of ['edit.cut', 'edit.copy', 'edit.delete'] as const) {
      expect(commandEnabled(command, IDLE_STATE)).toBe(false);
      expect(commandEnabled(command, with_({ vectors: 1 }))).toBe(true);
      expect(commandEnabled(command, with_({ components: 1 }))).toBe(true);
    }
  });

  it('needs something copied to paste and a history to undo or redo', () => {
    expect(commandEnabled('edit.paste', IDLE_STATE)).toBe(false);
    expect(commandEnabled('edit.paste', with_({ clipboard: 3 }))).toBe(true);
    expect(commandEnabled('edit.undo', with_({ canRedo: true }))).toBe(false);
    expect(commandEnabled('edit.undo', with_({ canUndo: true }))).toBe(true);
    expect(commandEnabled('edit.redo', with_({ canUndo: true }))).toBe(false);
    expect(commandEnabled('edit.redo', with_({ canRedo: true }))).toBe(true);
  });

  it('follows the job: sides need a double sided job, wrapping a rotary one', () => {
    for (const command of [
      'edit.swapSides',
      'view.multiSided',
      'view.sideTop',
      'view.sideBottom',
    ] as const) {
      expect(commandEnabled(command, IDLE_STATE)).toBe(false);
      expect(commandEnabled(command, with_({ doubleSided: true }))).toBe(true);
    }
    expect(commandEnabled('view.autoWrap', IDLE_STATE)).toBe(false);
    expect(commandEnabled('view.autoWrap', with_({ rotary: true }))).toBe(true);
  });

  it('greys the Model menu items that act on the selected components without one', () => {
    for (const command of [
      'model.bake',
      'model.boundary',
      'model.distort',
      'model.emboss',
      'model.exportClipart',
      'model.properties',
      'model.sculpt',
      'model.smooth',
    ] as const) {
      expect(commandEnabled(command, with_({ vectors: 3 }))).toBe(false);
      expect(commandEnabled(command, with_({ components: 1 }))).toBe(true);
    }
    for (const command of ['model.resetInside', 'model.resetOutside', 'model.split'] as const) {
      expect(commandEnabled(command, with_({ components: 1 }))).toBe(false);
      expect(commandEnabled(command, with_({ vectors: 1 }))).toBe(false);
      expect(commandEnabled(command, with_({ components: 1, vectors: 1 }))).toBe(true);
    }
    for (const command of ['model.replaceBelow', 'model.slice', 'model.exportStl'] as const) {
      expect(commandEnabled(command, IDLE_STATE)).toBe(true);
    }
  });

  it('greys the move toggles while 2D previews are off', () => {
    const off = with_({ draw2dPreviews: false });
    expect(commandEnabled('toolpaths.drawRapids', off)).toBe(false);
    expect(commandEnabled('toolpaths.drawJoins', off)).toBe(false);
    expect(commandEnabled('toolpaths.draw2dPreviews', off)).toBe(true);
  });

  it('keeps the rule table to the documented commands', () => {
    expect(gatedCommands().sort()).toEqual(
      [
        'edit.copy',
        'edit.cut',
        'edit.delete',
        'edit.paste',
        'edit.redo',
        'edit.swapSides',
        'edit.undo',
        'file.openDataFolder',
        'toolpaths.drawJoins',
        'toolpaths.drawPlunges',
        'toolpaths.drawRapids',
        'toolpaths.drawRetracts',
        'toolpaths.saveTemplateSelected',
        'view.autoWrap',
        'view.multiSided',
        'view.sideBottom',
        'view.sideTop',
        'model.bake',
        'model.boundary',
        'model.distort',
        'model.emboss',
        'model.exportClipart',
        'model.properties',
        'model.resetInside',
        'model.resetOutside',
        'model.sculpt',
        'model.smooth',
        'model.split',
      ].sort(),
    );
  });
});

describe('toolEnabled', () => {
  it('enables every icon that has a form or a command, whatever is selected', () => {
    expect(toolEnabled({ form: 'offset' })).toBe(true);
    expect(toolEnabled({ command: 'align.lastLeft' })).toBe(true);
    expect(toolEnabled({ command: 'edit.undo' })).toBe(true);
    expect(toolEnabled({})).toBe(false);
  });
});
