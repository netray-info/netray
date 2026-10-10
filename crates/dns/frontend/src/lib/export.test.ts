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

const HOSTILE = '"><img src=x onerror=alert(1)>[x](javascript:alert(1))';

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

interface Span {
  content: string;
  fence: number;
}

/** CommonMark inline code spans: a fence of N backticks closes at the next run of exactly N. */
function parseCodeSpans(md: string): { spans: Span[]; rest: string } {
  const spans: Span[] = [];
  let rest = '';
  let i = 0;
  while (i < md.length) {
    if (md[i] !== '`') {
      rest += md[i++];
      continue;
    }
    let n = 0;
    while (md[i + n] === '`') n++;
    let j = i + n;
    let close = -1;
    while (j < md.length) {
      if (md[j] !== '`') {
        j++;
        continue;
      }
      let m = 0;
      while (md[j + m] === '`') m++;
      if (m === n) {
        close = j;
        break;
      }
      j += m;
    }
    if (close < 0) {
      rest += md.slice(i, i + n);
      i += n;
      continue;
    }
    let content = md.slice(i + n, close).replace(/\n/g, ' ');
    if (content.length > 1 && content.startsWith(' ') && content.endsWith(' ') && content.trim() !== '') {
      content = content.slice(1, -1);
    }
    // inside a GFM table cell `\|` is an escaped pipe, also within a code span
    content = content.replace(/\\\|/g, '|');
    spans.push({ content, fence: n });
    i = close + n;
  }
  return { spans, rest };
}

function dataRow(md: string): string {
  const row = md.split('\n').find((l) => l.startsWith('|') && l.includes('example.com.') && !l.startsWith('| Name'));
  expect(row, 'table data row').toBeDefined();
  return row as string;
}

describe('toMarkdown code spans', () => {
  it('puts a hostile TXT value and the domain only inside code spans', () => {
    const md = toMarkdown([txtBatch(HOSTILE)], { query: 'example.com TXT', stats: null });
    const row = dataRow(md);
    const { spans } = parseCodeSpans(row);
    expect(spans.map((s) => s.content)).toContain(HOSTILE);
    expect(spans.map((s) => s.content)).toContain('example.com.');

    const { rest } = parseCodeSpans(md);
    expect(rest).not.toContain('<img');
    expect(rest).not.toContain('javascript:');
    expect(rest).not.toContain('](');
  });

  it('keeps a value with backticks verbatim inside a longer fence', () => {
    const value = 'a `b` c';
    const md = toMarkdown([txtBatch(value)], { query: 'example.com TXT', stats: null });
    const { spans } = parseCodeSpans(dataRow(md));
    const span = spans.find((s) => s.content === value);
    expect(span, 'span carrying the value verbatim').toBeDefined();
    expect(span!.fence).toBeGreaterThan(1);
  });
});
