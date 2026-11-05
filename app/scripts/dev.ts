// `pnpm dev`: the interface in a browser against the real kernel. Builds the
// kernel dev host (release, so geometry is fast), starts it, then starts
// Vite, which proxies /kernel to it. Ctrl+C stops both.
//
// Ports: VITE_PORT (default 1420) and CARVE_DEVHOST_PORT (default 1421), so
// several sessions can run side by side.

import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../..', import.meta.url));
const app = fileURLToPath(new URL('..', import.meta.url));
const vitePort = process.env.VITE_PORT ?? '1420';
const kernelPort = process.env.CARVE_DEVHOST_PORT ?? '1421';

const build = spawnSync('cargo', ['build', '--release', '-p', 'carve-devhost'], {
  cwd: root,
  stdio: 'inherit',
});
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

const env = { ...process.env, VITE_PORT: vitePort, CARVE_DEVHOST_PORT: kernelPort };
const children: ChildProcess[] = [
  spawn('cargo', ['run', '--release', '-q', '-p', 'carve-devhost'], {
    cwd: root,
    env,
    stdio: 'inherit',
  }),
  spawn('npx', ['vite'], { cwd: app, env, stdio: 'inherit' }),
];

let stopping = false;
function stop(code: number): void {
  if (stopping) {
    return;
  }
  stopping = true;
  for (const child of children) {
    child.kill('SIGTERM');
  }
  process.exit(code);
}

for (const child of children) {
  child.on('exit', (code: number | null) => stop(code ?? 0));
}
process.on('SIGINT', () => stop(0));
process.on('SIGTERM', () => stop(0));
console.log(`CarveStudio: http://localhost:${vitePort} (kernel on ${kernelPort})`);
