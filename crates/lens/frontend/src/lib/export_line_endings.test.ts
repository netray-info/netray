import { describe, it, expect } from 'vitest';
import { toMarkdown } from './export';

function dnsInput(messages: string[]) {
  return {
    domain: 'example.com',
    dns: { status: 'warn', headline: 'DNS', detail_url: 'https://example.com/dns', checks: [{ name: 'mx', verdict: 'warn', messages }] },
    tls: null, http: null, email: null, ip: null, summary: null, done: null,
  } as any;
}

describe('lens toMarkdown line endings in messages', () => {
  it('a message with \\n does not start a block-level line', () => {
    const md = toMarkdown(dnsInput(['x\n<div>y</div>']));
    expect(md).not.toContain('\r');
    for (const line of md.split('\n')) {
      expect(line.trimStart().startsWith('<div')).toBe(false);
    }
  });

  it('a message with a lone \\r leaves no \\r in the output', () => {
    const md = toMarkdown(dnsInput(['a\rb']));
    expect(md).not.toContain('\r');
  });
});
