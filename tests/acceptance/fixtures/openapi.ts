import Ajv from 'ajv';
import { expect } from '@playwright/test';

const ajv = new Ajv({ strict: false, allErrors: true });

// Minimal OpenAPI 3.x structure validation
const OPENAPI_SCHEMA = {
  type: 'object',
  required: ['openapi', 'info', 'paths'],
  properties: {
    openapi: { type: 'string', pattern: '^3\\.' },
    info: {
      type: 'object',
      required: ['title', 'version'],
      properties: {
        title: { type: 'string' },
        version: { type: 'string' },
      },
    },
    paths: { type: 'object' },
  },
};

const validate = ajv.compile(OPENAPI_SCHEMA);

export function assertValidOpenApi(json: unknown): void {
  const valid = validate(json);
  if (!valid) {
    const errors = validate.errors?.map(e => `${e.instancePath} ${e.message}`).join('; ');
    expect.soft(valid, `OpenAPI validation failed: ${errors}`).toBe(true);
  }
}
