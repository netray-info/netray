// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render } from 'solid-js/web';
import type { InspectResponse } from '../lib/types';

const copyToClipboard = vi.fn(async (_text: string) => true);
vi.mock('@netray-info/common-frontend/utils', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@netray-info/common-frontend/utils')>()),
  copyToClipboard: (t: string) => copyToClipboard(t),
  downloadFile: vi.fn(),
}));

import ExportButtons from './ExportButtons';

// Contract: tlsight's Markdown export puts every server-supplied value (quality detail,
// certificate subject, error message) and the hostname in an inline code span, so a
// copied report carries no live link, autolink or HTML.

const HOSTILE = '"><img src=x onerror=alert(1)>[x](javascript:alert(1))';

interface Parsed {
  spans: string[];
  rest: string;
}

function parseCodeSpans(md: string): Parsed {
  const spans: string[] = [];
  let rest = '';
  let i = 0;
  while (i < md.length) {
    const ch = md[i];
    if (ch === '\\' && i + 1 < md.length) {
      rest += ch + md[i + 1];
      i += 2;
      continue;
    }
    if (ch !== '`') {
      rest += ch;
      i++;
      continue;
    }
    let n = 0;
    while (md[i + n] === '`') n++;
    let j = i + n;
    let close = -1;
    while (j < md.length) {
      if (md[j] === '`') {
        let m = 0;
        while (md[j + m] === '`') m++;
        if (m === n) {
          close = j;
          break;
        }
        j += m;
      } else {
        j++;
      }
    }
    if (close < 0) {
      rest += md.slice(i, i + n);
      i += n;
      continue;
    }
    let content = md.slice(i + n, close).replace(/\n/g, ' ');
    if (content.length > 2 && content.startsWith(' ') && content.endsWith(' ') && content.trim() !== '') {
      content = content.slice(1, -1);
    }
    spans.push(content);
    rest += ' ';
    i = close + n;
  }
  return { spans, rest };
}

const result = {
  hostname: 'example.com',
  summary: { verdict: 'pass', checks: { chain_trusted: 'pass' } },
  ports: [
    {
      port: 443,
      ips: [
        {
          ip: '192.0.2.1',
          ip_version: 'v4',
          chain: [{ subject: HOSTILE }],
          error: { message: HOSTILE },
        },
      ],
    },
  ],
  quality: { verdict: 'warn', checks: [{ id: 'x', category: 'protocol', status: 'warn', label: 'Check', detail: HOSTILE }] },
  duration_ms: 12,
} as unknown as InspectResponse;

describe('tlsight Markdown export', () => {
  it('puts hostile values and the hostname only inside code spans', async () => {
    const el = document.createElement('div');
    document.body.appendChild(el);
    const dispose = render(() => <ExportButtons result={result} />, el);
    el.querySelector<HTMLButtonElement>('[aria-label="Copy as Markdown"]')!.click();
    await vi.waitFor(() => expect(copyToClipboard).toHaveBeenCalled());
    dispose();

    const md = copyToClipboard.mock.calls[0][0];
    const { spans, rest } = parseCodeSpans(md);

    expect(spans.filter((s) => s === HOSTILE).length).toBeGreaterThanOrEqual(3);
    expect(rest).not.toContain('<img');
    expect(rest).not.toContain('javascript:');
    expect(rest).not.toContain('onerror');
    expect(spans).toContain('example.com');
    expect(rest.split('\n')[0]).not.toContain('example.com');
  });
});
