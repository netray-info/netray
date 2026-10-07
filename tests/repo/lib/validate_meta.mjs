// Usage: node validate_meta.mjs <schema.json> <meta.json> [--set a.b.c=value ...]
// Validates <meta.json> (after applying --set overrides) against <schema.json>.
// Exit 0 valid, 1 invalid (errors on stderr), 2 usage/IO error.
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";

const require = createRequire(import.meta.url);
const [schemaPath, metaPath, ...rest] = process.argv.slice(2);
if (!schemaPath || !metaPath) {
  console.error("usage: validate_meta.mjs <schema> <json> [--set path=value ...]");
  process.exit(2);
}

let schema, meta;
try {
  schema = JSON.parse(readFileSync(schemaPath, "utf8"));
  meta = JSON.parse(readFileSync(metaPath, "utf8"));
} catch (e) {
  console.error(`cannot read inputs: ${e.message}`);
  process.exit(2);
}

for (let i = 0; i < rest.length; i++) {
  if (rest[i] !== "--set" || rest[i + 1] === undefined) {
    console.error(`bad argument: ${rest[i]}`);
    process.exit(2);
  }
  const arg = rest[++i];
  const eq = arg.indexOf("=");
  const keys = arg.slice(0, eq).split(".");
  const last = keys.pop();
  let node = meta;
  for (const k of keys) node = node[k];
  node[last] = arg.slice(eq + 1);
}

const Ajv = require("ajv");
const ajv = new Ajv({ allErrors: true });
const validate = ajv.compile(schema);
if (validate(meta)) process.exit(0);
for (const err of validate.errors) {
  console.error(`${err.dataPath || "/"} ${err.message} ${JSON.stringify(err.params)}`);
}
process.exit(1);
