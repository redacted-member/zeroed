// Covers the window while a long kernel command runs, so the pointer reaches
// nothing until it ends or Esc stops it (the status bar shows its progress).

import type { JSX } from 'react';

import { useBusyStore } from '../../state/busyStore';
import './BusyShield.scss';

export function BusyShield(): JSX.Element | null {
  const busy = useBusyStore((state) => state.busy);
  return busy ? <div aria-hidden="true" className="busy-shield" /> : null;
}
