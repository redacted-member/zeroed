// The app carries one version: app/package.json holds it, tauri.conf.json
// reads it from there, the Cargo workspace (and so the kernel's version in
// Help > About) must match it, and CHANGELOG.md has a section for it.
// tools/release/set-version.sh changes all of them at once.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

function read(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

const packageVersion = (JSON.parse(read('../../package.json')) as { version: string }).version;

describe('app version', () => {
  it('is a plain release number in package.json', () => {
    expect(packageVersion).toMatch(/^\d+\.\d+\.\d+$/);
  });

  it('is read by tauri.conf.json from package.json', () => {
    const conf = JSON.parse(read('../../src-tauri/tauri.conf.json')) as { version: string };
    expect(conf.version).toBe('../package.json');
  });

  it('matches the Cargo workspace version', () => {
    const cargo = read('../../../Cargo.toml');
    const section = cargo.slice(cargo.indexOf('[workspace.package]'));
    const match = /^version = "([^"]+)"$/m.exec(section);
    expect(match?.[1]).toBe(packageVersion);
  });

  it('has a section in CHANGELOG.md', () => {
    const changelog = read('../../../CHANGELOG.md');
    expect(changelog).toMatch(new RegExp(`^## ${packageVersion.replaceAll('.', '\\.')} `, 'm'));
  });
});
