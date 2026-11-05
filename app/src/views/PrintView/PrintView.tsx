import type { JSX } from 'react';

import { strings } from '../../lib/strings';
import { useDialogStore } from '../../state/dialogStore';

import './PrintView.scss';

/** The printed page: the captured 2D view alone. Hidden on screen. */
export function PrintView(): JSX.Element | null {
  const image = useDialogStore((state) => state.printImage);
  if (image === null) {
    return null;
  }
  return (
    <figure className="print-view">
      <img alt={strings.printing.imageAlt} className="print-view__image" src={image} />
    </figure>
  );
}
