import { describe, it, expect } from 'vitest';
import { serverSuggestions, helpServerRows, systemResolversAllowed } from './servers';

describe('@system visibility follows the server config', () => {
  const rows: { name: string; allowSystem: boolean | undefined; shown: boolean }[] = [
    { name: 'C3: config off', allowSystem: false, shown: false },
    { name: 'C4: config on', allowSystem: true, shown: true },
    { name: 'C5: config not answered or failed', allowSystem: undefined, shown: false },
  ];

  for (const { name, allowSystem, shown } of rows) {
    it(`${name}: autocomplete suggestions`, () => {
      const labels = serverSuggestions(allowSystem).map((s) => s.label);
      expect(labels.includes('@system')).toBe(shown);
      expect(labels).toContain('@cloudflare');
      expect(labels).toContain('@google');
    });

    it(`${name}: help rows`, () => {
      const tokens = helpServerRows(allowSystem).map((r) => r.token);
      expect(tokens.includes('@system')).toBe(shown);
      expect(tokens).toContain('@cloudflare');
      expect(tokens).toContain('@google');
    });
  }
});

describe('systemResolversAllowed', () => {
  const cases: [string, unknown, boolean][] = [
    ['true', { allow_system_resolvers: true }, true],
    ['false', { allow_system_resolvers: false }, false],
    ['missing key', {}, false],
    ['null body', null, false],
    ['undefined body', undefined, false],
    ['string "true"', { allow_system_resolvers: 'true' }, false],
  ];
  for (const [name, cfg, expected] of cases) {
    it(`${name} -> ${expected}`, () => {
      expect(systemResolversAllowed(cfg)).toBe(expected);
    });
  }
});
