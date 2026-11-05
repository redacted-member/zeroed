import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { startCloseGuard, startFileOpening } from './lib/fileCommands';
import { AppShell } from './shell/AppShell/AppShell';

import './styles/global.scss';

const container = document.getElementById('root');
if (container === null) {
  throw new Error('the page is missing its root element');
}

// The development tools (window.carveDev) load on their own, so neither
// they nor the form commands they expose weigh on the start bundle.
if (import.meta.env.DEV) {
  void import('./lib/devtools').then(({ installDevTools }) => installDevTools());
}
startFileOpening();
startCloseGuard();

createRoot(container).render(
  <StrictMode>
    <AppShell />
  </StrictMode>,
);
