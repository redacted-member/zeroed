// Rebuilds crates/carve-core/assets/samples from docs/testing/projects: inch
// units, round inch stock and depths, inch tools and toolpaths recalculated. Run a dev kernel first
// (CARVE_DEVHOST_PORT=1485), back up prefs.json, then:
//   node tools/make-samples.mjs
import { readFileSync } from 'node:fs';
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

const shipped = JSON.parse(
  readFileSync(`${ROOT}/crates/carve-core/assets/tools/default.json`, 'utf8'),
);
const fromDatabase = (name) => {
  const stored = shipped.tools.find((t) => t.name === name);
  if (stored === undefined) {
    throw new Error(`no tool ${name}`);
  }
  const { group: _group, ...tool } = stored;
  return tool;
};

const INCH = 25.4;

// Round inch stock for each sample. A design with joints keeps its size and sits
// on stock a little larger, with its joints made again for the new thickness;
// any other design scales with the job.
const STOCK = {
  'house-sign': { width: 16, height: 8, thickness: 0.75 },
  'relief-plaque': { width: 8, height: 6, thickness: 1 },
  'box-joints': { width: 24, height: 20, thickness: 0.5, joints: true },
  'knockdown-table': { width: 36, height: 28, thickness: 0.75, joints: true },
  'nested-sheet': { width: 24, height: 16, thickness: 0.5 },
  'two-sided': { width: 12, height: 8, thickness: 0.75 },
  'rotary-sleeve': { length: 8, diameter: 2.5 },
  'laser-coaster': { width: 12, height: 8, thickness: 0.125 },
};

const INCH_TOOLS = {
  'End Mill 6 mm': 'End Mill 1/4 inch',
  'End Mill 3 mm': 'End Mill 1/8 inch',
  'Ball Nose 3 mm': 'Ball Nose 1/8 inch',
  'Ball Nose 1.5 mm': 'Ball Nose 1/16 inch',
  'V Bit 60 deg': 'V Bit 60 deg 1/2 inch',
  'V Bit 90 deg': 'V Bit 90 deg 1/2 inch',
  'Diode Laser 0.1 mm': 'Diode Laser 0.004 inch',
};

function inchTool(tool) {
  const name = INCH_TOOLS[tool.name];
  return name === undefined ? tool : fromDatabase(name);
}

/** The tool in a toolpath's own name, with the name's metric size in inches. */
function inchName(name, tool) {
  return name.replace(/\s*\[[^\]]* mm\]/, ` [${tool.name}]`).replace(/ \d+(\.\d+)? mm$/, '');
}

const nearest = (mm, step) => Math.round(mm / INCH / step) * step * INCH;
/** To the nearest 1/16 inch, and never down to nothing. */
const sixteenths = (mm) => (mm === 0 ? 0 : Math.max(nearest(mm, 1 / 16), INCH / 32));
/** To the nearest five thousandths of an inch, and never down to nothing. */
const thousandths = (mm) => (mm === 0 ? 0 : Math.max(nearest(mm, 0.005), 0.005 * INCH));

const LENGTHS = new Set(['flatDepth', 'startDepth', 'length', 'radius', 'distance']);
const FINE = new Set(['allowance', 'restBoundaryOffset']);

/** A cut that goes through keeps its overcut below the new thickness. */
function depthIn(mm, from, to) {
  return mm >= from ? to + thousandths(mm - from) : sixteenths(mm);
}

/** Every tool in a request's settings in inches, and its lengths round inches. */
function inchSettings(value, from, to) {
  if (Array.isArray(value)) {
    return value.map((item) => inchSettings(item, from, to));
  }
  if (value === null || typeof value !== 'object') {
    return value;
  }
  if ('toolType' in value) {
    return inchTool(value);
  }
  const entries = Object.entries(value).map(([key, item]) => {
    if (typeof item !== 'number') {
      return [key, inchSettings(item, from, to)];
    }
    if (key === 'cutDepth') {
      return [key, item === 0 ? 0 : depthIn(item, from, to)];
    }
    if (key === 'thickness') {
      return [key, sixteenths(item)];
    }
    if (LENGTHS.has(key)) {
      return [key, sixteenths(item)];
    }
    if (FINE.has(key)) {
      return [key, thousandths(item)];
    }
    if (key === 'lineInterval' || key === 'minimumDetail') {
      return [key, nearest(item, 0.001)];
    }
    if (key === 'speed') {
      return [key, nearest(item, 5)];
    }
    return [key, item];
  });
  return Object.fromEntries(entries);
}

async function setStock(sample, info) {
  const stock = STOCK[sample];
  const machine = {
    ...info.machine,
    safeZ: 0.25 * INCH,
    plungeGap: 0.1 * INCH,
    homeZ: INCH,
  };
  let job = { ...info, units: 'inch', machine };
  if (info.kind === 'rotary') {
    // The rotary spindle mounts along Y; the design turns with the blank.
    job = { ...job, rotary: { ...job.rotary, axis: 'y' } };
    await call('job.setFlipped', { job });
    job = {
      ...job,
      rotary: { ...job.rotary, length: stock.length * INCH, diameter: stock.diameter * INCH },
      width: Math.PI * stock.diameter * INCH,
      height: stock.length * INCH,
      thickness: (stock.diameter / 2) * INCH,
    };
    await call('job.setScaled', { job });
    return;
  }
  job = {
    ...job,
    width: stock.width * INCH,
    height: stock.height * INCH,
    thickness: stock.thickness * INCH,
  };
  if (stock.joints) {
    await call('job.set', job);
    const { settings } = await call('joints.list');
    await call('joints.setSettings', { settings });
  } else {
    await call('job.setScaled', { job });
    await resizeSheets(job);
  }
}

/** The other sheets take the new stock too, and scale their designs with it. */
async function resizeSheets(job) {
  const sheets = await call('sheets.list');
  for (const sheet of sheets.filter((s) => sheets.length > 1 && s.material !== null)) {
    await call('sheets.edit', {
      ids: [sheet.id],
      material: {
        ...sheet.material,
        width: job.width,
        height: job.height,
        thickness: job.thickness,
      },
      scaleDesign: !sheet.active,
    });
  }
}

const only = process.argv.slice(2);
for (const sample of SAMPLES.filter((s) => only.length === 0 || only.includes(s))) {
  await call('doc.open', { path: `${ROOT}/docs/testing/projects/${sample}.carve` });
  const info = await call('job.get');
  await setStock(sample, info);
  const after = await call('job.get');
  // Recalculating can replace the parts of a multi-tool toolpath, so each turn
  // reads the list afresh.
  const count = (await call('cam.list')).length;
  for (let i = 0; i < count; i += 1) {
    const { id } = (await call('cam.list'))[i];
    const { request } = await call('cam.get', { id });
    const inches = inchSettings(request, info.thickness, after.thickness);
    if (info.kind === 'rotary' && inches.params?.profilePass !== undefined) {
      inches.params.profilePass = false;
    }
    await call('cam.recalculate', {
      id,
      request: { ...inches, name: inchName(request.name, inches.tool) },
    });
  }
  const list = await call('cam.list');
  await call('doc.saveCopy', { path: `${ROOT}/crates/carve-core/assets/samples/${sample}.carve` });
  console.log(
    sample,
    `${(after.width / INCH).toFixed(3)} x ${(after.height / INCH).toFixed(3)} x ${(after.thickness / INCH).toFixed(3)}`,
    list.map((t) => `${t.name} (${t.toolName}): ${t.moves} moves${t.stale ? ' STALE' : ''}`).join('; '),
  );
}
