import { describe, it, expect } from 'vitest';
import { toJson, toMarkdown } from './export';

// Contract: toJson / toMarkdown take one object
// { domain, dns, tls, http, email, ip, summary, done }.
const http = {
  status: 'warn',
  headline: 'HTTP',
  detail_url: 'https://example.com/http',
  checks: [
    { name: 'hsts', verdict: 'pass', messages: ['max-age 31536000'] },
    { name: 'https_redirect', verdict: 'fail', messages: ['no redirect'] },
  ],
};
const email = {
  status: 'pass',
  headline: 'Email',
  grade: 'A',
  detail_url: 'https://example.com/email',
  checks: [{ name: 'email_authentication', verdict: 'pass', messages: ['ok'] }],
};

function input() {
  return { domain: 'example.com', dns: null, tls: null, http, email, ip: null, summary: null, done: null } as any;
}

describe('export covers http and email', () => {
  it('toJson contains http and email with their checks', () => {
    const j = JSON.parse((toJson as any)(input()));
    expect(j.http.checks.map((c: any) => c.name)).toEqual(['hsts', 'https_redirect']);
    expect(j.email.checks[0].name).toBe('email_authentication');
  });

  it('toMarkdown has HTTP and Email sections', () => {
    const md = (toMarkdown as any)(input());
    expect(md).toContain('## HTTP');
    expect(md).toContain('HSTS');
    expect(md).toContain('HTTPS Redirect');
    expect(md).toContain('## Email');
    expect(md).toContain('Authentication');
  });
});
