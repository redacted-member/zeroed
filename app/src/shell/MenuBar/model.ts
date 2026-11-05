// The menus of the application, in the order and names. The
// in-window menu bar (browser host) renders them, and the desktop build
// sends the same tree to the native menu (nativeMenu.ts, app/src-tauri/
// src/menu.rs), so both hosts always match. Pure: the caller supplies the
// context.

import type { GadgetSummary } from '../../bridge/gadgets';
import type { PreviewQuality } from '../../bridge/cam';
import { commandEnabled, type EnableState } from '../../lib/enableState';
import { comboLabel } from '../../lib/keyLabels';
import { strings } from '../../lib/strings';
import { SHORTCUTS, type AppCommand } from '../../lib/shortcuts';
import { TOOL_GROUPS } from '../LeftPanel/toolGroups';
import type { DisplayState } from './displayState';
import { menuIcon } from './menuIcons';

export type MenuAction =
  | { type: 'command'; command: AppCommand }
  | { type: 'openRecent'; index: number }
  | { type: 'gadget'; name: string }
  | { type: 'refreshGadgets' };

export interface MenuItem {
  kind: 'item';
  id: string;
  label: string;
  action: MenuAction;
  shortcut?: string;
  disabled?: boolean;
  /** Set for check and radio items. */
  checked?: boolean;
  /** Radio items are one of a set; check items toggle on their own. */
  radio?: boolean;
  /** Secondary text after the label. */
  note?: string;
  /** Tooltip. */
  hint?: string;
  /** The shell/ToolIcons id drawn in the gutter left of the label. */
  icon?: string;
  /** A disabled line that only tells something (No gadgets installed); it has no icon. */
  info?: boolean;
}

export interface MenuSeparator {
  kind: 'separator';
}

export interface Submenu {
  kind: 'submenu';
  id: string;
  label: string;
  entries: readonly MenuEntry[];
  /** The shell/ToolIcons id drawn in the gutter left of the label. */
  icon?: string;
}

export type MenuEntry = MenuItem | MenuSeparator | Submenu;

export type MenuId =
  'file' | 'edit' | 'model' | 'machine' | 'toolpaths' | 'view' | 'gadgets' | 'help';

export interface MenuSpec {
  id: MenuId;
  label: string;
  entries: readonly MenuEntry[];
}

export interface MenuContext {
  recentFiles: readonly string[];
  gadgets: readonly GadgetSummary[];
  gadgetsLoaded: boolean;
  gadgetFailure: string | null;
  /** Whether guide lines show in the 2D view. */
  guidesVisible: boolean;
  /** Whether the Toolpaths panel is pinned open. */
  toolpathsPinned: boolean;
  /** The quality Preview Toolpaths simulates at. */
  previewQuality: PreviewQuality;
  /** Whether the 3D view draws the material block. */
  showBlock: boolean;
  /** Whether the 3D view background is shaded (a gradient). */
  shadedBackground: boolean;
  /** The Toolpath Drawing and View display toggles. */
  display: DisplayState;
  /** What lib/enableState greys items by. */
  enable: EnableState;
  /** Shortcut labels use the macOS symbols. */
  mac: boolean;
}

/** Menu commands that no feature answers yet; their items stay listed, disabled. */
export const UNAVAILABLE_COMMANDS: ReadonlySet<AppCommand> = new Set<AppCommand>([
  'gadgets.install',
  'gadgets.shortcuts',
  'machine.installOfflinePackage',
  'machine.searchOnline',
  'machine.updatePostDatabase',
  'help.enterLicense',
  'help.onlineFaq',
  'help.userForum',
  'help.supportOnline',
  'help.userPortal',
  'help.postEditingGuide',
  'help.migrate',
  'help.checkUpdates',
  'help.kickstarter',
]);

const KEY_NAMES: Readonly<Record<string, string>> = {
  Delete: 'Del',
  Backspace: 'Backspace',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
};

/** The label of the first key bound to `command`, if any. */
export function shortcutLabel(command: AppCommand, mac: boolean): string | undefined {
  const shortcut = SHORTCUTS.find((s) => s.command === command);
  if (shortcut === undefined) {
    return undefined;
  }
  return comboLabel(shortcut, mac, KEY_NAMES);
}

const SEPARATOR: MenuSeparator = { kind: 'separator' };
const m = strings.menuBar;
const t = m.items;
const p = strings.menus;

function recentLabel(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

/** The items of the Gadgets menu: one per gadget, then Run Last and Refresh. */
export function gadgetEntries(ctx: MenuContext): MenuEntry[] {
  const entries: MenuEntry[] = [];
  if (ctx.gadgetFailure !== null) {
    entries.push({
      kind: 'item',
      id: 'gadgets.failure',
      label: ctx.gadgetFailure,
      action: { type: 'refreshGadgets' },
      disabled: true,
      info: true,
    });
  } else if (!ctx.gadgetsLoaded) {
    entries.push({
      kind: 'item',
      id: 'gadgets.loading',
      label: t.gadgetsLoading,
      action: { type: 'refreshGadgets' },
      disabled: true,
      info: true,
    });
  } else if (ctx.gadgets.length === 0) {
    entries.push({
      kind: 'item',
      id: 'gadgets.empty',
      label: strings.gadgets.empty,
      action: { type: 'refreshGadgets' },
      disabled: true,
      info: true,
    });
  }
  for (const gadget of ctx.gadgets) {
    entries.push({
      kind: 'item',
      id: `gadget.${gadget.name}`,
      label: gadget.title,
      action: { type: 'gadget', name: gadget.name },
      disabled: gadget.error !== undefined,
      note: gadget.origin === 'user' ? strings.gadgets.userGadget : undefined,
      hint: gadget.error ?? gadget.description,
      icon: 'gadget',
    });
  }
  return entries;
}

/**
 * Edit > Align Selected Objects, after its Alignment Tools item: the tools of the Design tab's Align Objects
 * group, in its order and with its names and commands, so the menu and the
 * icons align the same way (to the material or to the last selected object).
 */
function alignEntries(command: (cmd: AppCommand, label: string) => MenuItem): MenuItem[] {
  const group = TOOL_GROUPS.design.find((g) => g.id === 'alignObjects');
  return (group?.tools ?? []).flatMap((tool) =>
    tool.command === undefined ? [] : [command(tool.command, tool.label)],
  );
}

/** Builds the menus for the current state of the app. */
export function buildMenus(ctx: MenuContext): MenuSpec[] {
  const command = (
    cmd: AppCommand,
    label: string,
    extra: Partial<Omit<MenuItem, 'kind' | 'action' | 'label'>> = {},
  ): MenuItem => ({
    kind: 'item',
    id: cmd,
    label,
    action: { type: 'command', command: cmd },
    shortcut: shortcutLabel(cmd, ctx.mac),
    icon: menuIcon(cmd),
    disabled: UNAVAILABLE_COMMANDS.has(cmd) || !commandEnabled(cmd, ctx.enable) ? true : undefined,
    ...extra,
  });
  const check = (cmd: AppCommand, label: string, checked: boolean): MenuItem =>
    command(cmd, label, { checked });
  const radio = (cmd: AppCommand, label: string, checked: boolean): MenuItem =>
    command(cmd, label, { checked, radio: true });
  const submenu = (id: string, label: string, entries: MenuEntry[], icon?: string): Submenu => ({
    kind: 'submenu',
    id,
    label,
    entries,
    icon,
  });
  const q = ctx.previewQuality;
  const tp = ctx.display.toolpaths;
  const v3 = ctx.display.view3d;

  const recent: MenuEntry[] = ctx.recentFiles.map((path, index) => ({
    kind: 'item',
    id: `file.recent.${index}`,
    label: recentLabel(path),
    hint: path,
    icon: 'fileRecent',
    action: { type: 'openRecent', index },
  }));
  if (recent.length > 0) {
    recent.push(SEPARATOR);
  }
  recent.push(
    command('file.clearRecent', t.clearRecent, { disabled: ctx.recentFiles.length === 0 }),
  );

  return [
    {
      id: 'file',
      label: m.file,
      entries: [
        command('file.new', t.new),
        command('file.newFromTemplate', t.newFromTemplate),
        command('file.open', t.open),
        submenu('file.openRecent', t.openRecent, recent, 'fileRecent'),
        command('file.close', p.close),
        command('file.save', t.save),
        command('file.saveAs', t.saveAs),
        command('file.saveAsTemplate', t.saveAsTemplate),
        command('file.incrementSave', p.incrementSave),
        SEPARATOR,
        command('file.print', p.print),
        command('file.printPreview', p.printPreview),
        command('file.printSetup', p.printSetup),
        SEPARATOR,
        submenu(
          'file.import',
          p.import,
          [
            command('file.importVectorsBitmaps', p.importVectorsBitmaps),
            command('file.importModel', p.importModel),
            command('file.importToolpaths', p.importToolpaths),
          ],
          'importVectors',
        ),
        submenu(
          'file.exportMenu',
          p.export,
          [
            command('file.exportEps', p.exportEps),
            command('file.exportDxf', p.exportDxf),
            command('file.exportAi', p.exportAi),
            command('file.exportSvg', p.exportSvg),
            command('file.exportPdf', p.exportPdf),
          ],
          'exportVectors',
        ),
        SEPARATOR,
        command('file.openDataFolder', p.openDataFolder),
      ],
    },
    {
      id: 'edit',
      label: m.edit,
      entries: [
        command('edit.undo', t.undo),
        command('edit.redo', t.redo),
        SEPARATOR,
        command('edit.cut', t.cut),
        command('edit.copy', t.copy),
        command('edit.paste', t.paste),
        command('edit.delete', t.delete),
        SEPARATOR,
        submenu(
          'edit.selection',
          p.selection,
          [
            command('edit.selectAll', p.selectAllVectors),
            command('select.openVectors', p.selectOpenVectors),
            command('select.duplicates', p.selectDuplicates),
            command('select.currentLayer', p.selectCurrentLayer),
            command('edit.selectNone', p.unselectAll),
            command('select.vectorSelector', p.vectorSelector),
          ],
          'vectorSelection',
        ),
        submenu(
          'edit.align',
          p.align,
          [command('form.align', p.alignmentTools), SEPARATOR, ...alignEntries(command)],
          'align',
        ),
        command('edit.joinVectors', p.joinVectors),
        command('edit.curveFit', p.curveFit),
        command('edit.nest', p.nest),
        SEPARATOR,
        command('edit.jobSetup', p.jobSize),
        command('edit.swapSides', t.swapSides),
        command('edit.notes', p.notes),
        command('edit.documentVariables', p.documentVariables),
        SEPARATOR,
        command('view.snapSettings', p.snapOptions),
        command('app.preferences', p.options),
      ],
    },
    {
      id: 'model',
      label: m.model,
      entries: [
        submenu(
          'model.create',
          p.createComponent,
          [
            command('model.twoRailSweep', p.twoRailSweep),
            command('model.extrudeWeave', p.extrudeWeave),
            command('model.turnSpin', p.turnSpin),
            command('model.texturedArea', p.texturedArea),
            command('model.addZeroPlane', p.addZeroPlane),
          ],
          'createShape',
        ),
        submenu(
          'model.createFrom',
          p.createFrom,
          [
            command('model.bake', p.bake),
            command('model.fromVisible', p.fromVisible),
            command('model.addDraft', p.addDraft),
            command('model.offsetModel', p.offsetModel),
            command('model.slice', p.slice),
            command('model.fromPreview', p.fromPreview),
          ],
          'fromVisible',
        ),
        command('model.fromBitmap', p.fromBitmap),
        command('model.importModel', p.importModel),
        command('model.exportStl', p.exportStl),
        command('model.exportGrayscale', p.exportGrayscale),
        command('model.exportClipart', p.exportClipart),
        command('model.sculpt', p.sculpt),
        command('model.smooth', p.smooth),
        command('model.resetInside', p.resetInside),
        command('model.resetOutside', p.resetOutside),
        command('model.split', p.split),
        command('model.boundary', p.boundary),
        command('model.emboss', p.emboss),
        command('model.scaleZ', p.scaleZ),
        command('model.distort', p.distort),
        command('model.replaceBelow', p.replaceBelow),
        command('model.properties', p.properties),
      ],
    },
    {
      id: 'machine',
      label: m.machine,
      entries: [
        command('machine.configuration', p.machineConfiguration),
        command('machine.add', p.addMachine),
        command('machine.installOfflinePackage', p.installOfflinePackage),
        command('machine.searchOnline', p.searchMachineOnline),
        SEPARATOR,
        command('machine.postManagement', p.postManagement),
        command('machine.installPost', p.installPost),
        command('machine.updatePostDatabase', p.updatePostDatabase),
      ],
    },
    {
      id: 'toolpaths',
      label: m.toolpaths,
      entries: [
        command('toolpaths.showTab', p.showToolpathsTab, {
          checked: ctx.toolpathsPinned,
          shortcut: shortcutLabel('tabs.toolpaths', ctx.mac),
        }),
        command('toolpaths.recalculateAll', p.recalculateAll),
        submenu(
          'toolpaths.drawing',
          p.toolpathDrawing,
          [
            check('toolpaths.draw2dPreviews', p.draw2dPreviews, tp.draw2dPreviews),
            check('toolpaths.solidPreviews', p.solidPreviews, tp.solidPreviews),
            check('toolpaths.autoOpen3d', p.autoOpen3d, tp.autoOpen3d),
            check('toolpaths.drawRapids', p.drawRapids, tp.drawRapids),
            check('toolpaths.drawPlunges', p.drawPlunges, tp.drawPlunges),
            check('toolpaths.drawRetracts', p.drawRetracts, tp.drawRetracts),
            check('toolpaths.drawJoins', p.drawJoins, tp.drawJoins),
          ],
          'toolpathDrawing',
        ),
        submenu(
          'toolpaths.quality',
          p.previewQuality,
          [
            radio('preview.standard', p.qualityStandard, q === 'standard'),
            radio('preview.high', p.qualityHigh, q === 'high'),
            radio('preview.veryHigh', p.qualityVeryHigh, q === 'veryHigh'),
            radio('preview.extremelyHigh', p.qualityExtremelyHigh, q === 'extremelyHigh'),
            radio('preview.maximum', p.qualityMaximum, q === 'maximum'),
          ],
          'previewQuality',
        ),
        submenu(
          'toolpaths.templates',
          p.templates,
          [
            command('toolpaths.saveTemplateSelected', p.saveTemplateSelected),
            command('toolpaths.saveTemplateVisible', p.saveTemplateVisible),
            command('toolpaths.loadTemplate', p.loadTemplate),
          ],
          'loadTemplate',
        ),
        command('toolpaths.merge', p.merge),
        command('toolpaths.jobSheet', p.jobSheet),
        command('toolpaths.materials', p.materials),
        command('toolpaths.toolDatabase', p.toolDatabase),
      ],
    },
    {
      id: 'view',
      label: m.view,
      entries: [
        command('view.zoomToJob', p.zoomToDrawing),
        submenu(
          'view.guides',
          p.guideLines,
          [
            check('view.guidesVisible', p.guidesVisible, ctx.guidesVisible),
            command('view.clearGuides', p.deleteGuides),
            command('view.lockGuides', p.lockGuides),
            command('view.unlockGuides', p.unlockGuides),
          ],
          'guides',
        ),
        SEPARATOR,
        check('view.drawOrigin', p.drawOrigin, v3.drawOrigin),
        check('view.drawModelingPlane', p.drawModelingPlane, v3.drawModelingPlane),
        check('view3d.showBlock', p.drawMaterialBlock, ctx.showBlock),
        command('view.multiSided', p.multiSided, {
          checked: v3.multiSided,
          shortcut: shortcutLabel('view.multiSided', ctx.mac),
        }),
        command('view.autoWrap', p.autoWrap, {
          checked: v3.autoWrap,
        }),
        SEPARATOR,
        check('view.colorShaded', p.colorShaded, v3.colorShaded),
        check('view.shadedBackground', p.shadedBackground, ctx.shadedBackground),
        check('view.lightFollows', p.lightFollows, v3.lightFollows),
        check('view.shadowShading', p.shadowShading, v3.shadowShading),
        command('view.saveShadedImage', p.saveShadedImage),
        SEPARATOR,
        command('view.newFurniture', p.newFurniture),
        SEPARATOR,
        command('view.tileHorizontal', p.tileHorizontal),
        command('view.tileVertical', p.tileVertical),
        command('view.cascade', p.cascade),
      ],
    },
    {
      id: 'gadgets',
      label: m.gadgets,
      entries: [
        command('gadgets.install', p.installGadget),
        command('gadgets.shortcuts', p.gadgetShortcuts),
        SEPARATOR,
        command('gadgets.jointCreator', p.jointCreator),
        SEPARATOR,
        ...gadgetEntries(ctx),
      ],
    },
    {
      id: 'help',
      label: m.help,
      entries: [
        command('help.contents', p.helpContents),
        command('help.shortcuts', p.keyboardShortcuts),
        command('help.videoTutorials', p.videoTutorials),
        SEPARATOR,
        command('help.whatsNew', p.whatsNew),
        command('help.releaseNotes', p.releaseNotes),
        command('help.licences', p.licences),
        command('help.enterLicense', p.enterLicense),
        SEPARATOR,
        command('help.onlineFaq', p.onlineFaq),
        command('help.userForum', p.userForum),
        command('help.supportOnline', p.supportOnline),
        command('help.userPortal', p.userPortal),
        command('help.postEditingGuide', p.postEditingGuide),
        SEPARATOR,
        command('help.migrate', p.migrate),
        command('help.checkUpdates', p.checkUpdates),
        command('help.kickstarter', p.kickstarter),
        SEPARATOR,
        command('help.about', p.about),
      ],
    },
  ];
}

/** Every item of a menu tree, submenus flattened, separators dropped. */
export function flatItems(entries: readonly MenuEntry[]): MenuItem[] {
  return entries.flatMap((entry) =>
    entry.kind === 'item' ? [entry] : entry.kind === 'submenu' ? flatItems(entry.entries) : [],
  );
}
