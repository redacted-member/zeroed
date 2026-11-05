// The macOS document types live twice: bundle.fileAssociations in
// tauri.conf.json, and src-tauri/Info.plist, whose lists replace the
// generated ones in the bundle to add the Finder document icons. This keeps
// the two in step, and checks every icon the plist names is copied into the
// bundle's Resources.

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

const tauriDir = new URL('../../src-tauri/', import.meta.url);

interface Association {
  ext: string[];
  name: string;
  role: string;
  rank?: string;
  exportedType?: { identifier: string; conformsTo?: string[] };
}

interface Conf {
  bundle: {
    targets: string[];
    fileAssociations: Association[];
    macOS: { files: Record<string, string>; entitlements: string; hardenedRuntime: boolean };
  };
}

const conf = JSON.parse(
  readFileSync(fileURLToPath(new URL('tauri.conf.json', tauriDir)), 'utf8'),
) as Conf;
const plist = readFileSync(fileURLToPath(new URL('Info.plist', tauriDir)), 'utf8');

/** The text of the array under `key` at the top of the plist. */
function topArray(key: string): string {
  const start = plist.indexOf(`<key>${key}</key>`);
  expect(start).toBeGreaterThan(-1);
  const open = plist.indexOf('<array>', start);
  let depth = 0;
  const tag = /<(\/?)array>/g;
  tag.lastIndex = open;
  for (let match = tag.exec(plist); match !== null; match = tag.exec(plist)) {
    depth += match[1] === '' ? 1 : -1;
    if (depth === 0) return plist.slice(open + '<array>'.length, match.index);
  }
  throw new Error(`${key} is not closed`);
}

/** The top level dicts of an array's text (the entries hold nested dicts). */
function dicts(array: string): string[] {
  const found: string[] = [];
  let depth = 0;
  let from = 0;
  const tag = /<(\/?)dict>/g;
  for (let match = tag.exec(array); match !== null; match = tag.exec(array)) {
    if (match[1] === '') {
      if (depth === 0) from = match.index;
      depth += 1;
    } else {
      depth -= 1;
      if (depth === 0) found.push(array.slice(from, match.index));
    }
  }
  return found;
}

function stringOf(dict: string, key: string): string | undefined {
  return new RegExp(`<key>${key}</key>\\s*<string>([^<]*)</string>`).exec(dict)?.[1];
}

function stringsOf(dict: string, key: string): string[] {
  const list = new RegExp(`<key>${key}</key>\\s*<array>([\\s\\S]*?)</array>`).exec(dict)?.[1];
  return list === undefined
    ? []
    : [...list.matchAll(/<string>([^<]*)<\/string>/g)].map((m) => m[1] ?? '');
}

const exported = new Map(
  dicts(topArray('UTExportedTypeDeclarations')).map((dict) => [
    stringOf(dict, 'UTTypeIdentifier') ?? '',
    dict,
  ]),
);

const documentTypes = dicts(topArray('CFBundleDocumentTypes')).map((dict) => {
  const types = stringsOf(dict, 'LSItemContentTypes');
  const ext =
    types.length === 0
      ? stringsOf(dict, 'CFBundleTypeExtensions')
      : types.flatMap((type) => stringsOf(exported.get(type) ?? '', 'public.filename-extension'));
  return {
    name: stringOf(dict, 'CFBundleTypeName'),
    role: stringOf(dict, 'CFBundleTypeRole'),
    rank: stringOf(dict, 'LSHandlerRank'),
    ext,
    types,
    icon: stringOf(dict, 'CFBundleTypeIconFile'),
  };
});

describe('bundle document types', () => {
  it('lists the same types, roles, ranks and extensions in Info.plist as tauri.conf.json', () => {
    const fromConf = conf.bundle.fileAssociations.map((a) => ({
      name: a.name,
      role: a.role,
      rank: a.rank ?? 'Default',
      ext: a.ext,
      types: a.exportedType === undefined ? [] : [a.exportedType.identifier],
    }));
    const fromPlist = documentTypes.map(({ name, role, rank, ext, types }) => ({
      name,
      role,
      rank,
      ext,
      types,
    }));
    expect(fromPlist).toEqual(fromConf);
  });

  it('declares each exported type once, as the configuration does', () => {
    const identifiers = conf.bundle.fileAssociations.flatMap((a) =>
      a.exportedType === undefined ? [] : [a.exportedType.identifier],
    );
    expect([...exported.keys()]).toEqual(identifiers);
    for (const association of conf.bundle.fileAssociations) {
      if (association.exportedType === undefined) continue;
      const dict = exported.get(association.exportedType.identifier) ?? '';
      expect(stringsOf(dict, 'UTTypeConformsTo')).toEqual(association.exportedType.conformsTo);
      expect(stringOf(dict, 'UTTypeDescription')).toBe(association.name);
    }
  });

  it('gives jobs and clipart a document icon that ships in Resources', () => {
    const icons = new Map(documentTypes.map((type) => [type.ext.join(), type.icon]));
    expect(icons.get('carve')).toBe('carve-document.icns');
    expect(icons.get('carveclip')).toBe('carveclip-document.icns');
    for (const icon of [
      ...icons.values(),
      ...[...exported.values()].map((d) => stringOf(d, 'UTTypeIconFile')),
    ]) {
      if (icon === undefined) continue;
      const source = conf.bundle.macOS.files[`Resources/${icon}`];
      expect(source).toBeDefined();
      expect(existsSync(fileURLToPath(new URL(source ?? '', tauriDir)))).toBe(true);
    }
  });

  it('builds a disk image under the hardened runtime with the entitlements file', () => {
    expect(conf.bundle.targets).toEqual(['app', 'dmg']);
    expect(conf.bundle.macOS.hardenedRuntime).toBe(true);
    expect(existsSync(fileURLToPath(new URL(conf.bundle.macOS.entitlements, tauriDir)))).toBe(true);
  });
});
