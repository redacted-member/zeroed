import type { JSX } from 'react';

import { useDialogStore } from '../../state/dialogStore';
import { VectorSelector } from './VectorSelector';

/** Edit > Selection > Vector Selector: the selector on its own, selecting vectors in the 2D view. */
export function MenuVectorSelector(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'vectorSelector');
  const close = useDialogStore((state) => state.close);
  return <VectorSelector filter={null} forToolpath={false} onClose={close} open={open} />;
}
