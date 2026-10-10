import { describe, it, expect, vi } from 'vitest';
import type { BatchEvent } from '../components/ResultsTable';

// export.ts imports ResultsTable.tsx, which registers solid-js delegated events at
// module load and so needs a `window.document` even in the node environment.
vi.hoisted(() => {
  const doc = { addEventListener() {}, removeEventListener() {} };
  (globalThis as Record<string, unknown>).window = { document: doc };
  (globalThis as Record<string, unknown>).document = doc;
});

const { toMarkdown } = await import('./export');

function txtBatch(txt: string): BatchEvent {
  return {
    record_type: 'TXT',
    completed: 1,
    total: 1,
    lookups: [
      {
        query: { name: 'example.com.', record_type: 'TXT' },
        name_server: 'udp:192.0.2.53:53',
        result: {
          Response: {
            records: [{ name: 'example.com.', ttl: 300, type: 'TXT', data: { TXT: { txt_string: txt } } }],
            response_time: { secs: 0, nanos: 5_000_000 },
          },
        },
      },
    ],
  } as unknown as BatchEvent;
}

function dataRow(md: string): string {
  const row = md.split('\n').find((l) => l.startsWith('|') && l.includes('example.com.') && !l.startsWith('| Name'));
  expect(row, 'table data row').toBeDefined();
  return row as string;
}

describe('prism toMarkdown line endings and empty values', () => {
  it('a TXT value with a lone \\r stays one table row, markup only inside a code span', () => {
    const md = toMarkdown([txtBatch('a\r<div>y</div>')], { query: 'example.com TXT', stats: null });
    const row = dataRow(md);
    expect(row).not.toMatch(/[\r\n]/);
    expect(row).toMatch(/(`+)[^`]*<div>y<\/div>[^`]*\1/);
    expect(row.replace(/(`+)[^`]*\1/g, '')).not.toContain('<div>');
  });

  it('an empty TXT value yields a Value cell without backticks', () => {
    const md = toMarkdown([txtBatch('')], { query: 'example.com TXT', stats: null });
    const header = md.split('\n').find((l) => l.startsWith('| Name'))!;
    const cols = header.split('|').map((c) => c.trim());
    const vi = cols.indexOf('Value');
    expect(vi).toBeGreaterThan(-1);
    const cells = dataRow(md).split(/(?<!\\)\|/).map((c) => c.trim());
    expect(cells[vi]).not.toContain('`');
  });
});
