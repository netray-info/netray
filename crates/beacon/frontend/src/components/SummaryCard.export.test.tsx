// @vitest-environment jsdom
import { render, cleanup, fireEvent } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Category, CheckResult, SummaryEvent } from '../lib/types';

const copyToClipboard = vi.hoisted(() => vi.fn(async (_text: string) => true));
vi.mock('@netray-info/common-frontend/utils', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@netray-info/common-frontend/utils')>()),
  copyToClipboard,
  downloadFile: vi.fn(),
}));

import SummaryCard from './SummaryCard';

afterEach(cleanup);

// Contract: beacon's Markdown export puts every backend-supplied detail and the
// domain in an inline code span, so a copied report carries no live link,
// autolink or HTML.

interface Parsed {
  spans: string[];
  rest: string; // the Markdown with all code spans removed
}

// Minimal CommonMark inline code span parser: a fence of N backticks opens a span
// that runs to the next run of exactly N backticks. One leading+trailing space is
// stripped when both are present and the content is not all spaces.
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
    // look for a closing run of exactly n backticks
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

const HOSTILE = '"><img src=x onerror=alert(1)>[x](javascript:alert(1))';

describe('SummaryCard Markdown export', () => {
  it('puts hostile details and the domain only inside code spans', async () => {
    const summary: SummaryEvent = { type: 'summary', grade: 'C', verdicts: { spf: 'warn' }, duration_ms: 12 };
    const spf: CheckResult = {
      type: 'category',
      category: 'spf',
      verdict: 'warn',
      title: 'SPF',
      detail: HOSTILE,
      sub_checks: [{ name: 'spf_ok', verdict: 'warn', detail: HOSTILE }],
    };
    const categories = new Map<Category, CheckResult>([['spf', spf]]);

    const { getByLabelText } = render(() => (
      <SummaryCard summary={summary} domain="example.com" categories={categories} />
    ));
    fireEvent.click(getByLabelText('Copy as Markdown'));
    await vi.waitFor(() => expect(copyToClipboard).toHaveBeenCalled());

    const md = copyToClipboard.mock.calls[0][0] as string;
    const { spans, rest } = parseCodeSpans(md);

    expect(spans.filter((s) => s === HOSTILE).length).toBe(2);
    expect(spans).toContain('example.com');
    expect(rest).not.toContain('<img');
    expect(rest).not.toContain('onerror');
    expect(rest).not.toContain('javascript:');
    expect(rest).not.toContain('](javascript');
    expect(rest).not.toContain('example.com');
  });
});
