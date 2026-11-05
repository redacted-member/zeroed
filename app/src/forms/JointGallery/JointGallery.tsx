import type { JSX } from 'react';

import type { JointKind } from '../../bridge/joints';
import { drawIcon } from '../../shell/ToolIcons/ToolIcons';

import './JointGallery.scss';

export interface GalleryItem<K extends string = JointKind> {
  kind: K;
  name: string;
  /** One line saying what the joint is for. */
  line: string;
  /** A shell/ToolIcons spec of the joint's picture. */
  icon: string;
}

interface JointGalleryProps<K extends string> {
  label: string;
  items: readonly GalleryItem<K>[];
  value: K;
  onChange: (kind: K) => void;
}

/**
 * The joint kinds as picture cards, a picture and a name each, with the
 * picked kind's one line under the grid (and every line as a tooltip).
 */
export function JointGallery<K extends string = JointKind>({
  label,
  items,
  value,
  onChange,
}: JointGalleryProps<K>): JSX.Element {
  return (
    <div className="joint-gallery">
      <div aria-label={label} className="joint-gallery__grid" role="radiogroup">
        {items.map((item) => (
          <button
            aria-checked={item.kind === value}
            className="joint-gallery__card"
            key={item.kind}
            onClick={() => onChange(item.kind)}
            role="radio"
            data-tip={item.line}
            type="button"
          >
            <span aria-hidden="true" className="joint-gallery__picture">
              {drawIcon(item.icon)}
            </span>
            <span className="joint-gallery__name">{item.name}</span>
          </button>
        ))}
      </div>
      <p className="joint-gallery__line">{items.find((item) => item.kind === value)?.line}</p>
    </div>
  );
}
