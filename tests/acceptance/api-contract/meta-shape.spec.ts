import { test, expect } from '@playwright/test';
import Ajv from 'ajv';
import { readFileSync } from 'node:fs';
import { toolOrigins } from '../fixtures/env.js';

// Every service's /api/meta matches the shared ecosystem-meta schema. Catches a service
// that ships an empty ecosystem (e.g. its config is not read) or a key lens no longer knows.
const schema = JSON.parse(
  readFileSync(new URL('../schemas/ecosystem-meta.schema.json', import.meta.url), 'utf8'),
);
const validate = new Ajv({ strict: false, allErrors: true }).compile(schema);

for (const { name, url } of toolOrigins()) {
  test(`${name}: /api/meta matches the ecosystem-meta schema`, async ({ request }) => {
    const response = await request.get(`${url}/api/meta`);
    expect(response.status(), `${url}/api/meta`).toBe(200);
    const body = await response.json();
    const ok = validate(body);
    expect(ok, `${url}/api/meta: ${JSON.stringify(validate.errors)}`).toBe(true);
  });
}
