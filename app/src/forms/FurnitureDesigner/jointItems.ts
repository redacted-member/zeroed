// Names, lines and pictures of the joints a furniture connection can take:
// the Joint Creator's own, plus the butt joint and the kinds the furniture
// work adds. Pictures are shell/ToolIcons specs in the Joint Creator
// gallery's style (two parts seen from the side, the first outlined).

import type { FurnitureJoint } from '../../bridge/furniture';
import type { JointKind } from '../../bridge/joints';
import { furnitureStrings } from '../../lib/strings/furniture';
import { jointsStrings } from '../../lib/strings/joints';
import type { GalleryItem } from '../JointGallery/JointGallery';
import { JOINT_ICONS } from '../JointCreatorForm/jointIcons';

const EXTRA_ICONS: Readonly<Record<Exclude<FurnitureJoint, JointKind>, string>> = {
  // A board standing on another, two screws through the lower one.
  none: 'M2 14h20v6H2Z;n:M7 3h10v11H7Z;t:M10 20V9M14 20V9;M9 20h2M13 20h2',
  // A dado that stops short: the groove ends before the front.
  stoppedDado: 'M2 14h8v3h4v-3h8v7H2Z;n:M10 3h4v14h-4Z;t:M17 14v3h-3',
};

/** The picture spec of a joint. */
export function jointIcon(joint: FurnitureJoint): string {
  if (joint in EXTRA_ICONS) {
    return EXTRA_ICONS[joint as keyof typeof EXTRA_ICONS];
  }
  return JOINT_ICONS[joint as JointKind] ?? EXTRA_ICONS.none;
}

/** The name and one line of a joint. */
export function jointText(joint: FurnitureJoint): { name: string; line: string } {
  const extra = furnitureStrings.furniture.joints;
  if (joint in extra) {
    return extra[joint as keyof typeof extra];
  }
  return jointsStrings.joints.kinds[joint as JointKind];
}

/** The gallery cards for these joints. */
export function galleryItems(joints: readonly FurnitureJoint[]): GalleryItem<FurnitureJoint>[] {
  return joints.map((joint) => ({
    kind: joint,
    ...jointText(joint),
    icon: jointIcon(joint),
  }));
}
