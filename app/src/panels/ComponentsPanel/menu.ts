// Right click menus of the component tree: a component (or the picked
// components), a group and a level. Pure so the contents can be tested.

import type { ComponentGroup } from '../../bridge/componentTree';
import {
  MIRROR_MODES,
  type Combine,
  type ComponentEditOp,
  type ComponentSummary,
  type Level,
  type MirrorMode,
} from '../../bridge/model';
import { strings } from '../../lib/formStrings';
import { withIcons, type PopupEntry } from '../../views/PopupMenu/model';

/** The icons of the component tree menus, by item id: the Modeling Tools icons where one runs the same. */
const ICONS: Readonly<Record<string, string>> = {
  properties: 'properties',
  rename: 'rename',
  visible: 'visible',
  group: 'group',
  ungroup: 'ungroup',
  transform: 'move',
  bake: 'bakeComponents',
  replaceBelow: 'replaceBelow',
  saveClipart: 'exportModel',
  newLevel: 'layers',
  fromVisible: 'fromVisible',
  boundary: 'componentBoundary',
  exportModel: 'exportModel',
  delete: 'editDelete',
  removeFromGroup: 'ungroup',
  reimport: 'importModel',
  fromSelected: 'fromVisible',
  createBrush: 'sculpt',
  copyToOtherSide: 'otherSideCopy',
  moveToOtherSide: 'otherSideMove',
  makeActive: 'layerActive',
  clipping: 'clip',
  clip: 'clip',
  updateClip: 'recalculateAll',
  clearClip: 'editDelete',
  up: 'arrowUp',
  down: 'arrowDown',
  'combine-add': 'combineAdd',
  'combine-subtract': 'combineSubtract',
  'combine-mergeHigh': 'combineHigh',
  'combine-mergeLow': 'combineLow',
  'combine-multiply': 'combineMultiply',
  'edit-smooth': 'smoothComponents',
  'edit-draft': 'addDraft',
  'edit-tilt': 'componentTilt',
  'edit-fade': 'componentFade',
  'edit-mirror': 'mirror',
  'edit-flip': 'componentFlip',
  'edit-scale': 'scaleModelHeight',
  'edit-emboss': 'embossComponent',
  'mirror-none': 'mirrorNone',
  'mirror-leftToRight': 'mirrorLeftToRight',
  'mirror-rightToLeft': 'mirrorRightToLeft',
  'mirror-topToBottom': 'mirrorTopToBottom',
  'mirror-bottomToTop': 'mirrorBottomToTop',
  'mirror-topLeft': 'mirrorTopLeft',
  'mirror-topRight': 'mirrorTopRight',
  'mirror-bottomLeft': 'mirrorBottomLeft',
  'mirror-bottomRight': 'mirrorBottomRight',
};

export type ComponentAction =
  | { kind: 'properties' }
  | { kind: 'rename' }
  | { kind: 'toggleVisible' }
  | { kind: 'combine'; combine: Combine }
  | { kind: 'level'; level: number }
  | { kind: 'group' }
  | { kind: 'removeFromGroup' }
  | { kind: 'edit'; op: ComponentEditOp }
  | { kind: 'fromSelected' }
  | { kind: 'bake' }
  | { kind: 'replaceBelow' }
  | { kind: 'transform' }
  | { kind: 'saveClipart' }
  | { kind: 'createBrush' }
  | { kind: 'reimport' }
  | { kind: 'otherSide'; copy: boolean }
  | { kind: 'delete' };

export type GroupAction =
  | { kind: 'properties' }
  | { kind: 'toggleVisible' }
  | { kind: 'combine'; combine: Combine }
  | { kind: 'ungroup' }
  | { kind: 'delete' };

export type LevelAction =
  | { kind: 'properties' }
  | { kind: 'toggleVisible' }
  | { kind: 'combine'; combine: Combine }
  | { kind: 'makeActive' }
  | { kind: 'clip' }
  | { kind: 'updateClip' }
  | { kind: 'clearClip' }
  | { kind: 'mirror'; mirror: MirrorMode | null }
  | { kind: 'up' }
  | { kind: 'down' }
  | { kind: 'newLevel' }
  | { kind: 'fromVisible' }
  | { kind: 'boundary' }
  | { kind: 'exportModel' }
  | { kind: 'delete' };

export type CombineChoices = ReadonlyArray<{ value: Combine; label: string }>;

export interface ComponentMenuContext {
  component: ComponentSummary;
  /** How many components are picked, this one included. */
  picked: number;
  levels: readonly Level[];
  combines: CombineChoices;
  edits: ReadonlyArray<{ op: ComponentEditOp; label: string }>;
  /** The job has two sides, so components can go to the other one. */
  doubleSided?: boolean;
}

const m = strings.panelMenus;
const t = strings.model.treeMenu;

function combineEntries<A>(
  current: Combine,
  combines: CombineChoices,
  action: (combine: Combine) => A,
): PopupEntry<A>[] {
  return combines.map((choice) => ({
    id: `combine-${choice.value}`,
    label: m.combine(choice.label),
    checked: current === choice.value,
    action: action(choice.value),
  }));
}

export function buildComponentMenu({
  component,
  picked,
  levels,
  combines,
  edits,
  doubleSided = false,
}: ComponentMenuContext): PopupEntry<ComponentAction>[] {
  const single = picked <= 1;
  const entries: PopupEntry<ComponentAction>[] = [
    { id: 'properties', label: t.properties, action: { kind: 'properties' } },
    { id: 'rename', label: m.rename, disabled: !single, action: { kind: 'rename' } },
    {
      id: 'visible',
      label: component.visible ? m.hide : m.show,
      action: { kind: 'toggleVisible' },
    },
    { separator: true },
    ...combineEntries<ComponentAction>(component.combine, combines, (combine) => ({
      kind: 'combine',
      combine,
    })),
    { separator: true },
    { id: 'group', label: t.group, action: { kind: 'group' } },
  ];
  if ((component.group ?? 0) !== 0) {
    entries.push({
      id: 'removeFromGroup',
      label: t.removeFromGroup,
      action: { kind: 'removeFromGroup' },
    });
  }
  if (levels.length > 1) {
    entries.push({ separator: true });
    for (const level of levels) {
      entries.push({
        id: `level-${level.id}`,
        label: m.moveToLevel(level.name),
        icon: 'moveToLayer',
        checked: component.level === level.id,
        action: { kind: 'level', level: level.id },
      });
    }
  }
  entries.push({ separator: true });
  for (const edit of edits) {
    entries.push({
      id: `edit-${edit.op}`,
      label: t.edit(edit.label),
      disabled: !single,
      action: { kind: 'edit', op: edit.op },
    });
  }
  entries.push(
    { id: 'transform', label: m.transform, disabled: !single, action: { kind: 'transform' } },
    ...(component.imported === true
      ? [
          {
            id: 'reimport',
            label: t.reimport,
            disabled: !single,
            action: { kind: 'reimport' as const },
          },
        ]
      : []),
    { separator: true },
    { id: 'fromSelected', label: t.fromSelected, action: { kind: 'fromSelected' } },
    { id: 'bake', label: m.bake, action: { kind: 'bake' } },
    {
      id: 'replaceBelow',
      label: m.replaceBelow,
      disabled: !single,
      action: { kind: 'replaceBelow' },
    },
    {
      id: 'saveClipart',
      label: m.saveClipart,
      disabled: !single,
      action: { kind: 'saveClipart' },
    },
    {
      id: 'createBrush',
      label: t.createBrush,
      disabled: !single,
      action: { kind: 'createBrush' },
    },
    ...(doubleSided
      ? [
          { separator: true as const },
          {
            id: 'copyToOtherSide',
            label: t.copyToOtherSide,
            action: { kind: 'otherSide' as const, copy: true },
          },
          {
            id: 'moveToOtherSide',
            label: t.moveToOtherSide,
            action: { kind: 'otherSide' as const, copy: false },
          },
        ]
      : []),
    { separator: true },
    { id: 'delete', label: m.delete, action: { kind: 'delete' } },
  );
  return withIcons(entries, ICONS);
}

export function buildGroupMenu(
  group: ComponentGroup,
  combines: CombineChoices,
): PopupEntry<GroupAction>[] {
  return withIcons<GroupAction>(
    [
      { id: 'properties', label: t.properties, action: { kind: 'properties' } },
      {
        id: 'visible',
        label: group.visible ? m.hide : m.show,
        action: { kind: 'toggleVisible' },
      },
      { separator: true },
      ...combineEntries<GroupAction>(group.combine, combines, (combine) => ({
        kind: 'combine',
        combine,
      })),
      { separator: true },
      { id: 'ungroup', label: t.ungroup, action: { kind: 'ungroup' } },
      { id: 'delete', label: m.delete, action: { kind: 'delete' } },
    ],
    ICONS,
  );
}

export interface LevelMenuContext {
  level: Level;
  /** Place in combine order, 0 for the base level. */
  index: number;
  count: number;
  active: boolean;
  /** Closed vectors are selected in the drawing. */
  vectorsSelected: boolean;
  combines: CombineChoices;
}

export function buildLevelMenu({
  level,
  index,
  count,
  active,
  vectorsSelected,
  combines,
}: LevelMenuContext): PopupEntry<LevelAction>[] {
  const clipped = (level.clip ?? []).length > 0;
  return withIcons<LevelAction>(
    [
      { id: 'properties', label: t.properties, action: { kind: 'properties' } },
      {
        id: 'visible',
        label: level.visible ? m.hide : m.show,
        action: { kind: 'toggleVisible' },
      },
      {
        id: 'makeActive',
        label: t.makeActive,
        checked: active,
        action: { kind: 'makeActive' },
      },
      { separator: true },
      ...combineEntries<LevelAction>(level.combine, combines, (combine) => ({
        kind: 'combine',
        combine,
      })),
      { separator: true },
      {
        id: 'clipping',
        label: t.clipping,
        entries: [
          { id: 'clip', label: t.clip, disabled: !vectorsSelected, action: { kind: 'clip' } },
          {
            id: 'updateClip',
            label: t.updateClip,
            disabled: !clipped,
            action: { kind: 'updateClip' },
          },
          {
            id: 'clearClip',
            label: t.clearClip,
            disabled: !clipped,
            action: { kind: 'clearClip' },
          },
        ],
      },
      { separator: true },
      {
        id: 'mirror-none',
        label: t.mirror(t.mirrorModes.none),
        checked: (level.mirror ?? null) === null,
        action: { kind: 'mirror', mirror: null },
      },
      ...MIRROR_MODES.map((mode) => ({
        id: `mirror-${mode}`,
        label: t.mirror(t.mirrorModes[mode]),
        checked: level.mirror === mode,
        action: { kind: 'mirror' as const, mirror: mode },
      })),
      { separator: true },
      // The tree lists levels bottom up: up is later in combine order.
      { id: 'up', label: t.moveUp, disabled: index === count - 1, action: { kind: 'up' } },
      { id: 'down', label: t.moveDown, disabled: index === 0, action: { kind: 'down' } },
      { id: 'newLevel', label: t.newLevel, action: { kind: 'newLevel' } },
      { id: 'fromVisible', label: t.fromVisible, action: { kind: 'fromVisible' } },
      { id: 'boundary', label: t.boundary, action: { kind: 'boundary' } },
      { id: 'exportModel', label: t.exportModel, action: { kind: 'exportModel' } },
      { separator: true },
      { id: 'delete', label: t.deleteLevel, disabled: count < 2, action: { kind: 'delete' } },
    ],
    ICONS,
  );
}
