import type { JSX } from 'react';

import type { TextAnchor } from '../../bridge/document';
import { strings } from '../../lib/formStrings';

import './TextAnchorField.scss';

/** The nine anchors, row by row from the top left. */
export const TEXT_ANCHORS: readonly TextAnchor[] = [
  'topLeft',
  'topCentre',
  'topRight',
  'middleLeft',
  'centre',
  'middleRight',
  'bottomLeft',
  'bottomCentre',
  'bottomRight',
];

interface TextAnchorFieldProps {
  /** The picked anchor; null for a text placed by its first baseline. */
  value: TextAnchor | null;
  onChange: (anchor: TextAnchor) => void;
  name: string;
}

/** Nine radio buttons laid out on the corners, edges and centre of a box. */
export function TextAnchorField({ value, onChange, name }: TextAnchorFieldProps): JSX.Element {
  const text = strings.text.create;
  return (
    <div aria-label={text.anchor} className="text-anchor-field" role="radiogroup">
      <span className="text-anchor-field__label">{text.anchor}</span>
      <div className="text-anchor-field__grid">
        {TEXT_ANCHORS.map((anchor) => (
          <input
            aria-label={text.anchors[anchor]}
            checked={value === anchor}
            className="text-anchor-field__point"
            key={anchor}
            name={name}
            onChange={() => onChange(anchor)}
            data-tip={text.anchors[anchor]}
            type="radio"
          />
        ))}
      </div>
    </div>
  );
}
