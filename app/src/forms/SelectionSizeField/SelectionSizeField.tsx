import type { JSX } from 'react';

import type { Units } from '../../bridge/types';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';

import './SelectionSizeField.scss';

const text = strings.editObjects.selectionSize;

/** Read-only width and height of the selection, as the layout tools show first. */
export function SelectionSizeField({ units }: { units: Units }): JSX.Element {
  const bbox = useDocumentStore((store) => store.selection.bbox);
  const size = (value: number) => `${lengthText(value, units)} ${unitSuffix(units)}`;
  return (
    <fieldset className="selection-size">
      <legend className="selection-size__legend">{text.title}</legend>
      <p className="selection-size__value" role="status">
        {bbox === null ? text.none : text.size(size(bbox[2] - bbox[0]), size(bbox[3] - bbox[1]))}
      </p>
    </fieldset>
  );
}
