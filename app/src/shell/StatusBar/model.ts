import { strings } from '../../lib/strings';
import type { KernelStatus } from '../../state/documentStore';

/**
 * The status bar's left line: "Ready", or the picked tool's usage hint
 * when there is one. A kernel still starting or not responding says so
 * over the hint, since nothing works until it is back.
 */
export function statusLine(status: KernelStatus, hint: string | null): string {
  switch (status) {
    case 'ready':
      return hint ?? strings.statusBar.kernelReady;
    case 'connecting':
      return strings.statusBar.kernelConnecting;
    case 'offline':
    default:
      return strings.statusBar.kernelOffline;
  }
}
