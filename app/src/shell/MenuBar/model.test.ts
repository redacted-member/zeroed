import { beforeAll, describe, expect, it } from 'vitest';

import type { GadgetSummary } from '../../bridge/gadgets';
import { IDLE_STATE, type EnableState } from '../../lib/enableState';
import { MENU_COMMANDS } from '../../lib/menu';
import { defaultPrefs } from '../../lib/prefs';
import { TOOL_GROUPS } from '../LeftPanel/toolGroups';
import { loadExtraIcons } from '../ToolIcons/ToolIcons';
import { displayState, togglePatch } from './displayState';
import {
  UNAVAILABLE_COMMANDS,
  buildMenus,
  flatItems,
  gadgetEntries,
  shortcutLabel,
  type MenuContext,
  type MenuEntry,
  type MenuItem,
} from './model';

const CTX: MenuContext = {
  recentFiles: [],
  gadgets: [],
  gadgetsLoaded: true,
  gadgetFailure: null,
  guidesVisible: true,
  toolpathsPinned: true,
  previewQuality: 'standard',
  showBlock: true,
  shadedBackground: false,
  display: displayState(defaultPrefs(), false),
  enable: IDLE_STATE,
  mac: false,
};

/** Everything selected, copied and undoable, on a double sided rotary desktop job. */
const BUSY: EnableState = {
  vectors: 2,
  components: 1,
  canUndo: true,
  canRedo: true,
  clipboard: 1,
  toolpathSelected: true,
  draw2dPreviews: true,
  doubleSided: true,
  rotary: true,
  native: true,
};

const GEAR: GadgetSummary = {
  name: 'gear',
  title: 'Gear Maker',
  description: 'Draws a spur gear',
  origin: 'bundled',
};

/** The menu as labels, submenus as `Label > [...]`, separators as `-`. */
function outline(entries: readonly MenuEntry[]): unknown[] {
  return entries.map((entry) =>
    entry.kind === 'separator'
      ? '-'
      : entry.kind === 'submenu'
        ? { [entry.label]: outline(entry.entries) }
        : entry.label,
  );
}

function menu(id: string, ctx: MenuContext = CTX) {
  const found = buildMenus(ctx).find((m) => m.id === id);
  if (found === undefined) {
    throw new Error(`no ${id} menu`);
  }
  return found;
}

function item(id: string, ctx: MenuContext = CTX): MenuItem | undefined {
  return buildMenus(ctx)
    .flatMap((m) => flatItems(m.entries))
    .find((i) => i.id === id);
}

// The icons menu commands name load lazily in the app.
beforeAll(loadExtraIcons);

describe('buildMenus', () => {
  it('has the menus in order', () => {
    expect(buildMenus(CTX).map((m) => m.label)).toEqual([
      'File',
      'Edit',
      'Model',
      'Machine',
      'Toolpaths',
      'View',
      'Gadgets',
      'Help',
    ]);
  });

  it('lays out the File menu', () => {
    expect(outline(menu('file').entries)).toEqual([
      'New',
      'New from Template...',
      'Open...',
      { 'Open Recent': ['Clear Menu'] },
      'Close',
      'Save',
      'Save As...',
      'Save as Template...',
      'Increment and Save',
      '-',
      'Print...',
      'Print Preview',
      'Print Setup...',
      '-',
      {
        Import: [
          'Import Vectors / Bitmaps...',
          'Import Component / 3D Model...',
          'Import Toolpaths...',
        ],
      },
      { Export: ['EPS...', 'DXF...', 'AI...', 'SVG...', 'PDF...'] },
      '-',
      'Open Application Data Folder...',
    ]);
  });

  it('lays out the Edit menu', () => {
    expect(outline(menu('edit').entries)).toEqual([
      'Undo',
      'Redo',
      '-',
      'Cut',
      'Copy',
      'Paste',
      'Delete',
      '-',
      {
        Selection: [
          'Select All Vectors',
          'Select All Open Vectors',
          'Select All Duplicate Vectors',
          'Select All Vectors On Current Layer',
          'Unselect All',
          'Vector Selector...',
        ],
      },
      {
        'Align Selected Objects': [
          'Alignment Tools...',
          '-',
          'Center in Material',
          'Center in Material X only',
          'Center in Material Y only',
          'Align Left',
          'Align Right',
          'Align Top',
          'Align Bottom',
          'Align Center',
          'Align Horizontal Center',
          'Align Vertical Center',
          'Align Outside Left',
          'Align Outside Right',
          'Align Outside Top',
          'Align Outside Bottom',
        ],
      },
      'Join Vectors...',
      'Curve Fit Vectors...',
      'Nest Selected Vectors...',
      '-',
      'Job Size and Position...',
      'Swap Sides',
      'Notes...',
      'Document Variables...',
      '-',
      'Snap Options...',
      'Options...',
    ]);
  });

  it('lays out the Model menu: two submenus, then flat items', () => {
    const outlined = outline(menu('model').entries);
    expect(outlined.slice(0, 2).map((e) => Object.keys(e as object)[0])).toEqual([
      'Create Component',
      'Create Component From',
    ]);
    expect(outlined.slice(2)).toEqual([
      'Create Component from Bitmap...',
      'Import Component / 3D Model...',
      'Export as STL...',
      'Export as Grayscale Bitmap...',
      'Export as 3D Clipart...',
      'Sculpt Visible Model...',
      'Smooth Selected Components...',
      'Reset Inside Selected Vectors',
      'Reset Outside Selected Vectors',
      'Split Selected Component',
      'Create Vector Boundary Around Selected Components',
      'Emboss Component...',
      'Scale Model Z Height...',
      'Distort Selected Component...',
      'Replace Below...',
      'Selected Component Properties...',
    ]);
  });

  it('lays out Machine, Toolpaths, View and Help', () => {
    expect(outline(menu('machine').entries)).toEqual([
      'Machine Configuration...',
      'Add Machine...',
      'Install Offline Package...',
      'Search for Machine Online...',
      '-',
      'Post-Processor Management...',
      'Install Post-Processor...',
      'Update Post-Processor Database',
    ]);
    const toolpaths = outline(menu('toolpaths').entries);
    expect(toolpaths[0]).toBe('Show Toolpaths Tab');
    expect(toolpaths.slice(-4)).toEqual([
      'Merge Visible Toolpaths...',
      'Create Job Sheet...',
      'Materials...',
      'Tool Database...',
    ]);
    const view = flatItems(menu('view').entries).map((i) => i.label);
    expect(view[0]).toBe('Zoom to Drawing');
    expect(view.slice(-3)).toEqual([
      'Tile Windows Horizontal',
      'Tile Windows Vertical',
      'Cascade Windows',
    ]);
    expect(outline(menu('help').entries)).toEqual([
      'Help Contents',
      'Keyboard Shortcuts',
      'Tutorial Browser...',
      '-',
      "What's New",
      'Release Notes',
      'Third Party Licenses',
      'Enter License Code...',
      '-',
      'View the Online FAQ...',
      'View the User Forum...',
      'Visit Support Online...',
      'Visit the User Portal...',
      'Post Processor Editing Guide',
      '-',
      'Migrate From Older Version...',
      'Check for Updates',
      'Run Kickstarter',
      '-',
      'About CarveStudio',
    ]);
    const labels = flatItems(menu('view').entries).map((i) => i.label);
    expect(labels).not.toContain('Save 2D View as Image...');
    expect(flatItems(menu('edit').entries).map((i) => i.id)).not.toContain('edit.xor');
  });

  it('uses only known menu command ids, each once', () => {
    const commands = buildMenus(CTX)
      .flatMap((m) => flatItems(m.entries))
      .flatMap((i) => (i.action.type === 'command' ? [i.action.command] : []));
    // Cut, copy and paste are native items in the desktop menu.
    const known = new Set<string>([...MENU_COMMANDS, 'edit.cut', 'edit.copy', 'edit.paste']);
    expect(commands.filter((c) => !known.has(c))).toEqual([]);
    expect(new Set(commands).size).toBe(commands.length);
  });

  it('disables the items no feature answers yet, and only those', () => {
    for (const entry of buildMenus({
      ...CTX,
      enable: BUSY,
      display: displayState(defaultPrefs(), true),
      recentFiles: ['/a.carve'],
    }).flatMap((m) => flatItems(m.entries))) {
      if (entry.action.type !== 'command') {
        continue;
      }
      expect(entry.disabled === true).toBe(UNAVAILABLE_COMMANDS.has(entry.action.command));
    }
  });

  it('shows the state of check and radio items', () => {
    expect(item('view.guidesVisible')?.checked).toBe(true);
    expect(item('view.guidesVisible', { ...CTX, guidesVisible: false })?.checked).toBe(false);
    expect(item('view3d.showBlock')?.checked).toBe(true);
    expect(item('toolpaths.showTab', { ...CTX, toolpathsPinned: false })?.checked).toBe(false);
    const high = item('preview.high', { ...CTX, previewQuality: 'high' });
    expect(high?.checked).toBe(true);
    expect(high?.radio).toBe(true);
    expect(item('preview.standard', { ...CTX, previewQuality: 'high' })?.checked).toBe(false);
    expect(item('view.shadedBackground', { ...CTX, shadedBackground: true })?.checked).toBe(true);
  });

  it('shows the drawing and view toggles from the preferences', () => {
    const prefs = defaultPrefs();
    prefs.toolpaths.drawJoins = false;
    prefs.toolpaths.solidPreviews = true;
    prefs.view3d.drawOrigin = true;
    prefs.view3d.colorShaded = false;
    const ctx = { ...CTX, display: displayState(prefs, false) };
    expect(item('toolpaths.drawJoins', ctx)?.checked).toBe(false);
    expect(item('toolpaths.drawRapids', ctx)?.checked).toBe(true);
    expect(item('toolpaths.solidPreviews', ctx)?.checked).toBe(true);
    expect(item('view.drawOrigin', ctx)?.checked).toBe(true);
    expect(item('view.colorShaded', ctx)?.checked).toBe(false);
    expect(item('preview.maximum', { ...CTX, previewQuality: 'maximum' })?.checked).toBe(true);
    expect(item('view.autoWrap')?.disabled).toBe(true);
    expect(
      item('view.autoWrap', { ...CTX, enable: { ...IDLE_STATE, rotary: true } })?.disabled,
    ).toBe(undefined);
  });

  it('flips one preference per toggle', () => {
    const prefs = defaultPrefs();
    expect(togglePatch('toolpaths.drawPlunges', prefs)).toEqual({
      toolpaths: { drawPlunges: false },
    });
    expect(togglePatch('view.lightFollows', prefs)).toEqual({ view3d: { lightFollows: true } });
    expect(togglePatch('view.cascade', prefs)).toBeNull();
  });

  it('enables what depends on the job, the host and the toolpath list', () => {
    const on = (enable: Partial<EnableState>) => ({ ...CTX, enable: { ...IDLE_STATE, ...enable } });
    expect(item('edit.swapSides')?.disabled).toBe(true);
    expect(item('edit.swapSides', on({ doubleSided: true }))?.disabled).toBeUndefined();
    expect(item('file.openDataFolder')?.disabled).toBe(true);
    expect(item('file.openDataFolder', on({ native: true }))?.disabled).toBeUndefined();
    expect(item('toolpaths.saveTemplateSelected')?.disabled).toBe(true);
    expect(
      item('toolpaths.saveTemplateSelected', on({ toolpathSelected: true }))?.disabled,
    ).toBeUndefined();
  });

  it('greys Undo, Redo, Cut, Copy, Paste and Delete by the selection and history', () => {
    for (const id of [
      'edit.undo',
      'edit.redo',
      'edit.cut',
      'edit.copy',
      'edit.paste',
      'edit.delete',
    ]) {
      expect(item(id)?.disabled).toBe(true);
      expect(item(id, { ...CTX, enable: BUSY })?.disabled).toBeUndefined();
    }
    const components = { ...CTX, enable: { ...IDLE_STATE, components: 1 } };
    expect(item('edit.copy', components)?.disabled).toBeUndefined();
    expect(item('edit.paste', components)?.disabled).toBe(true);
  });

  it('greys the move drawing toggles while 2D previews are off', () => {
    const off = { ...CTX, enable: { ...IDLE_STATE, draw2dPreviews: false } };
    for (const id of [
      'toolpaths.drawRapids',
      'toolpaths.drawPlunges',
      'toolpaths.drawRetracts',
      'toolpaths.drawJoins',
    ]) {
      expect(item(id)?.disabled).toBeUndefined();
      expect(item(id, off)?.disabled).toBe(true);
    }
    expect(item('toolpaths.draw2dPreviews', off)?.disabled).toBeUndefined();
  });

  it('lists the Align Objects tools under Align Selected Objects, with their commands', () => {
    const edit = menu('edit').entries.find((e) => e.kind === 'submenu' && e.id === 'edit.align');
    const align = edit?.kind === 'submenu' ? flatItems(edit.entries).slice(1) : [];
    const group = TOOL_GROUPS.design.find((g) => g.id === 'alignObjects');
    expect(align.map((i) => [i.label, i.action])).toEqual(
      (group?.tools ?? []).map((t) => [t.label, { type: 'command', command: t.command }]),
    );
    expect(align).toHaveLength(14);
    // Each shows the icon of its Align Objects tool in the gutter.
    expect(align.map((i) => i.icon)).toEqual((group?.tools ?? []).map((t) => t.id));
    const first = edit?.kind === 'submenu' ? edit.entries[0] : undefined;
    expect(first?.kind === 'item' ? [first.label, first.action, first.shortcut] : null).toEqual([
      'Alignment Tools...',
      { type: 'command', command: 'form.align' },
      'F10',
    ]);
    expect(first?.kind === 'item' ? first.icon : null).toBe('align');
  });

  it('lists recent files by name in Open Recent', () => {
    const recent = menu('file', {
      ...CTX,
      recentFiles: ['/jobs/sign.carve', 'C:\\a\\b.carve'],
    }).entries.find((entry) => entry.kind === 'submenu');
    const items = recent?.kind === 'submenu' ? flatItems(recent.entries) : [];
    expect(items.map((i) => i.label)).toEqual(['sign.carve', 'b.carve', 'Clear Menu']);
    expect(items[1]?.action).toEqual({ type: 'openRecent', index: 1 });
    expect(items[2]?.disabled).toBe(false);
    expect(item('file.clearRecent')?.disabled).toBe(true);
  });

  it('starts the Gadgets menu with Install and Shortcuts, the Joint Creator, then the gadgets', () => {
    const entries = menu('gadgets', { ...CTX, gadgets: [GEAR] }).entries;
    expect(outline(entries)).toEqual([
      'Install New Gadget...',
      'Gadget Shortcuts...',
      '-',
      'Joint Creator',
      '-',
      'Gear Maker',
    ]);
    // The Joint Creator item has its icon beside its label.
    const joint = flatItems(entries).find((i) => i.id === 'gadgets.jointCreator');
    expect(joint?.icon).toBe('jointCreator');
    expect(joint?.action).toEqual({ type: 'command', command: 'gadgets.jointCreator' });
  });
});

describe('gadgetEntries', () => {
  it('lists each gadget, disabling broken ones', () => {
    const items = flatItems(
      gadgetEntries({
        ...CTX,
        gadgets: [
          GEAR,
          { ...GEAR, name: 'bad', title: 'Broken', error: 'syntax error', origin: 'user' },
        ],
      }),
    );
    expect(items.map((i) => i.id)).toEqual(['gadget.gear', 'gadget.bad']);
    expect(items[0]?.action).toEqual({ type: 'gadget', name: 'gear' });
    expect(items[1]?.disabled).toBe(true);
    expect(items[1]?.note).toBe('user');
  });

  it('says when there are no gadgets, while loading and on failure', () => {
    expect(flatItems(gadgetEntries(CTX))[0]?.id).toBe('gadgets.empty');
    expect(flatItems(gadgetEntries({ ...CTX, gadgetsLoaded: false }))[0]?.id).toBe(
      'gadgets.loading',
    );
    const failed = flatItems(gadgetEntries({ ...CTX, gadgetFailure: 'no folder' }))[0];
    expect(failed?.label).toBe('no folder');
    expect(failed?.disabled).toBe(true);
  });
});

describe('shortcutLabel', () => {
  it('formats the first bound key for each platform', () => {
    expect(shortcutLabel('edit.undo', false)).toBe('Ctrl+Z');
    expect(shortcutLabel('edit.undo', true)).toBe('\u2318Z');
    expect(shortcutLabel('view.mode3d', false)).toBe('F3');
    expect(shortcutLabel('view.tileVertical', false)).toBe('PgUp');
    expect(shortcutLabel('file.saveAs', false)).toBeUndefined();
  });
});
