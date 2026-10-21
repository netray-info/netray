import { readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const HOSTS = ['lens', 'dns', 'tls', 'http', 'email', 'ip'];
export const CLASSES = ['spacing', 'typography', 'colour'];

const REPO_ROOT = fileURLToPath(new URL('../../../', import.meta.url)).replace(/\/$/, '');
const DEFAULT_EXPORT = 'export/canvas.html';
const DEFAULT_FIXTURES = 'tests/acceptance/fixtures/history';

const TOP = { required: [], optional: ['export', 'fixtures', 'boards', 'accepted'] };
const BOARD = {
  required: ['title', 'host', 'path', 'width', 'theme', 'elements'],
  optional: ['target', 'state', 'wait', 'click'],
};
const CLICK = { required: [], optional: ['board', 'page'] };
const ELEMENT = { required: ['name', 'board', 'page', 'class', 'properties'], optional: [] };
const ACCEPTED = { required: ['board', 'element', 'property', 'reason'], optional: [] };

export class ManifestError extends Error {}

export function slug(title) {
  return title
    .toLowerCase()
    .replace(/\(.*?\)/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

// A board's state names one page state of a canvas board: its files, rows and accepted entries
// carry it, so one board title can be measured in several states (v2-result-polish R14).
export function boardName(board) {
  return board.state ? `${board.title} [${board.state}]` : board.title;
}

export function boardSlug(board) {
  return board.state ? `${slug(board.title)}-${slug(board.state)}` : slug(board.title);
}

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const isString = (v) => typeof v === 'string' && v.length > 0;

function checkKeys(obj, spec, where) {
  const allowed = [...spec.required, ...spec.optional];
  for (const key of Object.keys(obj)) {
    if (!allowed.includes(key)) throw new ManifestError(`${where}: unknown key "${key}"`);
  }
  for (const key of spec.required) {
    if (!(key in obj)) throw new ManifestError(`${where}: missing ${key}`);
  }
}

function checkString(obj, key, where) {
  if (key in obj && !isString(obj[key])) {
    throw new ManifestError(`${where}: ${key} must be a non-empty string`);
  }
}

function checkElement(el, boardTitle, index, seen) {
  const label = isObject(el) && isString(el.name)
    ? `board "${boardTitle}" element "${el.name}"`
    : `board "${boardTitle}" element[${index}]`;
  if (!isObject(el)) throw new ManifestError(`${label}: must be an object`);
  checkKeys(el, ELEMENT, label);
  for (const key of ['name', 'board', 'page']) checkString(el, key, label);
  if (seen.has(el.name)) {
    throw new ManifestError(`board "${boardTitle}": duplicate element "${el.name}"`);
  }
  seen.add(el.name);
  if (!CLASSES.includes(el.class)) {
    throw new ManifestError(`${label}: class "${el.class}" is not one of ${CLASSES.join(', ')}`);
  }
  if (!Array.isArray(el.properties) || el.properties.length === 0 || !el.properties.every(isString)) {
    throw new ManifestError(`${label}: properties must be a non-empty array of strings`);
  }
}

function checkBoard(board, index) {
  const where = `boards[${index}]`;
  if (!isObject(board)) throw new ManifestError(`${where}: must be an object`);
  checkKeys(board, BOARD, where);
  for (const key of ['title', 'host', 'path', 'theme', 'target', 'state', 'wait']) checkString(board, key, where);
  if (!HOSTS.includes(board.host)) {
    throw new ManifestError(`${where}: host "${board.host}" is not one of ${HOSTS.join(', ')}`);
  }
  if (!board.path.startsWith('/')) throw new ManifestError(`${where}: path must start with "/"`);
  if (('target' in board) !== board.path.includes('{target}')) {
    throw new ManifestError(`${where}: a target needs "{target}" in the path, and "{target}" needs a target`);
  }
  if ('click' in board) {
    if (!isObject(board.click)) throw new ManifestError(`${where}: click must be an object`);
    checkKeys(board.click, CLICK, `${where} click`);
    for (const key of CLICK.optional) checkString(board.click, key, `${where} click`);
  }
  if (!Number.isInteger(board.width) || board.width <= 0) {
    throw new ManifestError(`${where}: width must be a positive integer`);
  }
  if (board.theme !== 'light' && board.theme !== 'dark') {
    throw new ManifestError(`${where}: theme must be light or dark`);
  }
  if (!Array.isArray(board.elements) || board.elements.length === 0) {
    throw new ManifestError(`${where}: elements must be a non-empty array`);
  }
  const seen = new Set();
  board.elements.forEach((el, i) => checkElement(el, board.title, i, seen));
}

function checkAccepted(entry, index) {
  const where = `accepted[${index}]`;
  if (!isObject(entry)) throw new ManifestError(`${where}: must be an object`);
  checkKeys(entry, ACCEPTED, where);
  for (const key of ACCEPTED.required) checkString(entry, key, where);
}

export function loadManifest(path) {
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(path, 'utf8'));
  } catch (err) {
    throw new ManifestError(`${path}: ${err.message}`);
  }
  if (!isObject(manifest)) throw new ManifestError('manifest: must be an object');
  checkKeys(manifest, TOP, 'manifest');
  for (const key of ['export', 'fixtures']) checkString(manifest, key, 'manifest');
  if (!Array.isArray(manifest.boards) || manifest.boards.length === 0) {
    throw new ManifestError('manifest: boards must be a non-empty array');
  }
  manifest.boards.forEach(checkBoard);
  // Output files are named by the title's slug: two boards on one slug would overwrite each other.
  const slugs = new Map();
  manifest.boards.forEach((b, i) => {
    const s = boardSlug(b);
    if (slugs.has(s)) {
      throw new ManifestError(`boards[${i}]: board "${boardName(b)}" names the same files as boards[${slugs.get(s)}] (${s})`);
    }
    slugs.set(s, i);
  });
  if ('accepted' in manifest) {
    if (!Array.isArray(manifest.accepted)) {
      throw new ManifestError('manifest: accepted must be an array');
    }
    manifest.accepted.forEach(checkAccepted);
    const keys = new Map();
    manifest.accepted.forEach((a, i) => {
      const key = JSON.stringify([a.board, a.element, a.property]);
      if (keys.has(key)) {
        throw new ManifestError(`accepted[${i}]: duplicate of accepted[${keys.get(key)}] (${a.board} / ${a.element} / ${a.property})`);
      }
      keys.set(key, i);
    });
  }
  return manifest;
}

export function resolveExport(manifest, manifestPath) {
  return resolve(dirname(resolve(manifestPath)), manifest.export ?? DEFAULT_EXPORT);
}

export function resolveFixtures(manifest) {
  const dir = manifest.fixtures ?? DEFAULT_FIXTURES;
  return isAbsolute(dir) ? dir : resolve(REPO_ROOT, dir);
}

function main(argv) {
  const usage = 'usage: manifest.mjs [--fixtures|--hosts] <manifest.json>';
  const flag = argv[0]?.startsWith('--') ? argv.shift() : null;
  if (argv.length !== 1 || (flag !== null && flag !== '--fixtures' && flag !== '--hosts')) {
    console.error(usage);
    return 2;
  }
  const path = argv[0];
  try {
    const manifest = loadManifest(path);
    if (flag === '--fixtures') console.log(resolveFixtures(manifest));
    else if (flag === '--hosts') {
      for (const host of new Set(manifest.boards.map((b) => b.host))) console.log(host);
    } else console.log(`manifest ok: ${path}`);
    return 0;
  } catch (err) {
    if (!(err instanceof ManifestError)) throw err;
    console.error(err.message.split('\n')[0]);
    return 2;
  }
}

function invokedDirectly() {
  try {
    return import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href;
  } catch {
    return false;
  }
}

if (process.argv[1] && invokedDirectly()) {
  process.exit(main(process.argv.slice(2)));
}
