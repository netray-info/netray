// @vitest-environment jsdom
import { render, cleanup, fireEvent } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';

const copyToClipboard = vi.fn(async (_text: string) => true);
vi.mock('@netray-info/common-frontend/utils', () => ({
  copyToClipboard: (text: string) => copyToClipboard(text),
  downloadFile: vi.fn(),
}));

import ExportButtons from './ExportButtons';
import type { InspectResponse } from '../lib/types';

afterEach(cleanup);

// Contract: spectra's Markdown export puts every message, header value, redirect
// location and the URL in an inline code span, so a copied report carries no live
// link, autolink or HTML.

interface Parsed {
  spans: string[];
  rest: string; // the Markdown with all code spans removed
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

const HOSTILE = '"><img src=x onerror=alert(1)>[x](javascript:alert(1))';

const hdr = (value: string | null = null) => ({ status: 'pass', value, message: null });

function hostileResult(): InspectResponse {
  return {
    url: 'https://example.com/',
    final_url: 'https://example.com/',
    status: 200,
    http_version: 'HTTP/2',
    duration_ms: 12,
    enrichment: { ip: '192.0.2.1', org: null, ip_type: null, threat: null },
    quality: {
      verdict: 'warn',
      checks: [{ id: 'x', label: 'Check', status: 'warn', message: HOSTILE }],
    },
    security: {
      hsts: { status: 'pass', max_age: 31536000, include_sub_domains: false, preload: false },
      csp: { status: 'pass', issues: [] },
      x_frame_options: hdr(HOSTILE),
      x_content_type_options: hdr(),
      referrer_policy: hdr(),
      permissions_policy: hdr(),
      coop: hdr(),
      coep: hdr(),
      corp: hdr(),
    },
    cookies: [],
    cors: {
      status: 'warn',
      allows_any_origin: false,
      reflects_origin: false,
      allows_credentials: false,
      message: HOSTILE,
    },
    redirects: [{ status: 301, url: 'https://example.com/', location: HOSTILE }],
  } as unknown as InspectResponse;
}

describe('spectra markdown export wraps untrusted values in inline code spans', () => {
  it('C12: hostile message, header value, redirect location, CORS message and the URL appear only inside code spans', async () => {
    copyToClipboard.mockClear();
    const { getByLabelText } = render(() => <ExportButtons result={hostileResult()} />);
    fireEvent.click(getByLabelText('Copy as Markdown'));
    await vi.waitFor(() => expect(copyToClipboard).toHaveBeenCalled());
    const md = copyToClipboard.mock.calls[0][0];

    const { spans, rest } = parseCodeSpans(md);
    expect(spans).toContain(HOSTILE);
    expect(rest).not.toContain('<img');
    expect(rest).not.toContain('javascript:');
    expect(rest).not.toContain('onerror');

    const heading = md.split('\n').find((l) => l.startsWith('# '))!;
    const h = parseCodeSpans(heading);
    expect(h.spans).toContain('https://example.com/');
    expect(h.rest).not.toContain('https://example.com/');
  });
});
