import { describe, it, expect } from 'vitest';
import { certDisplayName } from './cert';

describe('certDisplayName', () => {
  it('extracts CN from a full DN string', () => {
    expect(certDisplayName('CN=example.com, O=Example Inc, C=US')).toBe('example.com');
  });

  it('trims whitespace around CN value', () => {
    expect(certDisplayName('CN= example.com ,O=Test')).toBe('example.com');
  });

  it('returns the full string when no CN is present', () => {
    expect(certDisplayName('O=Example Inc, C=US')).toBe('O=Example Inc, C=US');
  });

  it('handles CN-only DN', () => {
    expect(certDisplayName('CN=Let\'s Encrypt Authority X3')).toBe("Let's Encrypt Authority X3");
  });

  it('handles empty string', () => {
    expect(certDisplayName('')).toBe('');
  });

  it('extracts first CN when multiple appear', () => {
    // CN= match stops at comma, so first CN is returned
    const result = certDisplayName('CN=leaf.com, CN=issuer.com');
    expect(result).toBe('leaf.com');
  });
});
