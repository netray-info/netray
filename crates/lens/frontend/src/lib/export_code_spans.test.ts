import { describe, it, expect } from 'vitest';
import { toMarkdown } from './export';

// Contract: lens's Markdown export puts every message and the domain in an inline
// code span, so a copied report carries no live link, autolink or HTML.

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

function dnsInput(messages: string[], domain = 'example.com') {
  return {
    domain,
    dns: { status: 'warn', headline: 'DNS', detail_url: 'https://example.com/dns', checks: [{ name: 'mx', verdict: 'warn', messages }] },
    tls: null, http: null, email: null, ip: null, summary: null, done: null,
  } as any;
}

function ipInput(messages: string[]) {
  return {
    domain: 'example.com',
    dns: null, tls: null, http: null, email: null,
    ip: { status: 'warn', headline: 'IP', detail_url: 'https://example.com/ip', checks: [{ name: 'blocklist', verdict: 'warn', messages }], addresses: [] },
    summary: null, done: null,
  } as any;
}

describe('markdown export wraps messages and domain in inline code spans', () => {
  it('C5: a hostile message appears only inside a code span, and so does the domain', () => {
    const hostile = '"><img src=x onerror=alert(1)>[x](javascript:alert(1))';
    for (const md of [toMarkdown(dnsInput([hostile])), toMarkdown(ipInput([hostile]))]) {
      const { spans, rest } = parseCodeSpans(md);
      expect(spans).toContain(hostile);
      expect(rest).not.toContain('<img');
      expect(rest).not.toContain('javascript:');
      expect(rest).not.toContain('onerror');
    }
    const md = toMarkdown(dnsInput([hostile]));
    const heading = md.split('\n').find((l) => l.startsWith('# '))!;
    const h = parseCodeSpans(heading);
    expect(h.spans).toContain('example.com');
    expect(h.rest).not.toContain('example.com');
  });

  it('C6: a host in a message is inside a code span, so it is not autolinked', () => {
    const msg = 'MX www.example.com. has no A/AAAA records';
    for (const md of [toMarkdown(dnsInput([msg])), toMarkdown(ipInput([msg]))]) {
      const { spans, rest } = parseCodeSpans(md);
      expect(spans).toContain(msg);
      expect(rest).not.toContain('www.example.com');
    }
  });

  it('C7: the fence is longer than any backtick run in the message and the message survives verbatim', () => {
    const single = 'value `x` here';
    const double = 'a ``y`` b';
    const md = toMarkdown(dnsInput([single, double]));
    const { spans, rest } = parseCodeSpans(md);
    expect(spans).toContain(single);
    expect(spans).toContain(double);
    expect(rest).not.toMatch(/`/);
    // the fence enclosing each message is longer than its longest backtick run
    for (const msg of [single, double]) {
      const longest = Math.max(...(msg.match(/`+/g) ?? ['']).map((r) => r.length));
      const fences = [...md.matchAll(/(`+) ?[^`\n]*?\1(?!`)/g)];
      const idx = md.indexOf(msg);
      expect(idx).toBeGreaterThan(0);
      let k = idx - 1;
      while (md[k] === ' ') k--;
      let n = 0;
      while (md[k - n] === '`') n++;
      expect(n).toBeGreaterThan(longest);
      void fences;
    }
  });
});
