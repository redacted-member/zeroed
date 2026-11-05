// The buttons along the top of the Component Tree, in order: an icon from
// shell/ToolIcons, the short label under it and the full name for the
// tooltip and screen readers. The panel wires each id to its action.

import { strings } from '../../lib/formStrings';

export type TreeTool =
  | 'moveUp'
  | 'moveDown'
  | 'newLevel'
  | 'group'
  | 'ungroup'
  | 'fromVisible'
  | 'fromVectors'
  | 'properties'
  | 'remove';

export interface TreeToolButton {
  id: TreeTool;
  icon: string;
  label: string;
  title: string;
}

const text = strings.model.tree;
const menu = strings.model.treeMenu;

export const TREE_TOOLS: readonly TreeToolButton[] = [
  { id: 'moveUp', icon: 'arrowUp', label: text.up, title: text.moveUp },
  { id: 'moveDown', icon: 'arrowDown', label: text.down, title: text.moveDown },
  { id: 'newLevel', icon: 'layerAdd', label: text.newLevel, title: menu.newLevel },
  { id: 'group', icon: 'group', label: text.group, title: menu.group },
  { id: 'ungroup', icon: 'ungroup', label: text.ungroup, title: menu.ungroup },
  { id: 'fromVisible', icon: 'fromVisible', label: text.fromVisible, title: menu.fromVisible },
  { id: 'fromVectors', icon: 'createShape', label: text.fromVectors, title: text.fromVectorsTitle },
  { id: 'properties', icon: 'properties', label: text.properties, title: text.propertiesTitle },
  { id: 'remove', icon: 'editDelete', label: text.remove, title: text.removeTitle },
];
