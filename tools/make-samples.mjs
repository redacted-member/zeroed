// Rebuilds crates/carve-core/assets/samples from docs/testing/projects: inch
// units, inch tools and toolpaths recalculated with them. Run a dev kernel first
// (CARVE_DEVHOST_PORT=1485), back up prefs.json, then:
//   node tools/make-samples.mjs
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PORT = process.env.CARVE_DEVHOST_PORT ?? '1485';
const SAMPLES = [
  'house-sign',
  'relief-plaque',
  'box-joints',
  'knockdown-table',
  'nested-sheet',
  'two-sided',
  'rotary-sleeve',
  'laser-coaster',
];

async function call(name, args = {}) {
  const response = await fetch(
    `http://127.0.0.1:${PORT}/kernel/call?name=${encodeURIComponent(name)}`,
    { method: 'POST', body: JSON.stringify(args) },
  );
  const reply = JSON.parse((await response.text()).replace(/^JSON/, ''));
  if (!reply.ok) {
    throw new Error(`${name}: ${JSON.stringify(reply.error)}`);
  }
  return reply.result;
}

const listed = await call('tools.list');
const database = listed.tools ?? listed.list ?? listed;
const fromDatabase = (name) => {
  const tool = database.find((t) => t.name === name);
  if (tool === undefined) {
    throw new Error(`no tool ${name}`);
  }
  return tool;
};

/** A database tool under a new name and diameter, for a size the database lacks. */
const sized = (base, name, diameter) => {
  const tool = structuredClone(base);
  tool.name = name;
  tool.toolType.diameter = diameter;
  return tool;
};

const INCH = 25.4;
function inchTool(tool) {
  switch (tool.name) {
    case 'End Mill 6 mm':
      return fromDatabase('End Mill 1/4 inch');
    case 'End Mill 3 mm':
      return fromDatabase('End Mill 1/8 inch');
    case 'Ball Nose 3 mm':
      return fromDatabase('Ball Nose 1/8 inch');
    case 'Ball Nose 1.5 mm':
      return sized(fromDatabase('Ball Nose 1/8 inch'), 'Ball Nose 1/16 inch', INCH / 16);
    case 'Diode Laser 0.1 mm':
      return sized(tool, 'Diode Laser 0.004 inch', 0.004 * INCH);
    default:
      return tool;
  }
}

/** The tool in a toolpath's own name, with the name's metric size in inches. */
function inchName(name, tool) {
  return name.replace(/\s*\[[^\]]* mm\]/, ` [${tool.name}]`).replace(/ \d+(\.\d+)? mm$/, '');
}

/** Every tool in a request's settings, such as rest tools, in inches. */
function inchTools(value) {
  if (Array.isArray(value)) {
    return value.map(inchTools);
  }
  if (value === null || typeof value !== 'object') {
    return value;
  }
  const mapped = Object.fromEntries(Object.entries(value).map(([k, v]) => [k, inchTools(v)]));
  return 'toolType' in mapped ? inchTool(mapped) : mapped;
}

const only = process.argv.slice(2);
for (const sample of SAMPLES.filter((s) => only.length === 0 || only.includes(s))) {
  await call('doc.open', { path: `${ROOT}/docs/testing/projects/${sample}.carve` });
  const info = await call('job.get');
  await call('job.set', { ...info, units: 'inch' });
  if (info.kind === 'rotary') {
    // The rotary spindle mounts along Y; the design turns with the blank.
    await call('job.setFlipped', {
      job: { ...info, units: 'inch', rotary: { ...info.rotary, axis: 'y' } },
    });
  }
  // Recalculating can replace the parts of a multi-tool toolpath, so each turn
  // reads the list afresh.
  const count = (await call('cam.list')).length;
  for (let i = 0; i < count; i += 1) {
    const { id } = (await call('cam.list'))[i];
    const { request } = await call('cam.get', { id });
    const inches = inchTools(request);
    await call('cam.recalculate', {
      id,
      request: { ...inches, name: inchName(request.name, inches.tool) },
    });
  }
  const after = await call('cam.list');
  await call('doc.saveCopy', { path: `${ROOT}/crates/carve-core/assets/samples/${sample}.carve` });
  console.log(
    sample,
    after.map((t) => `${t.name} (${t.toolName}): ${t.moves} moves${t.stale ? ' STALE' : ''}`).join('; '),
  );
}
