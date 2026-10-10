import { describe, it, expect } from 'vitest';
import { explainTrustReason, isUnknownIssuer } from './trust';

describe('explainTrustReason', () => {
  it('maps UnknownIssuer to human-readable text', () => {
    const result = explainTrustReason('UnknownIssuer');
    expect(result).toContain('trust store');
  });

  it('maps BadSignature to human-readable text', () => {
    const result = explainTrustReason('BadSignature');
    expect(result).toContain('invalid signature');
  });

  it('maps Expired to human-readable text', () => {
    const result = explainTrustReason('Expired');
    expect(result).toContain('expired');
  });

  it('maps NotValidYet to human-readable text', () => {
    const result = explainTrustReason('NotValidYet');
    expect(result).toContain('not yet valid');
  });

  it('maps Revoked to human-readable text', () => {
    const result = explainTrustReason('Revoked');
    expect(result).toContain('revoked');
  });

  it('maps BadEncoding to human-readable text', () => {
    const result = explainTrustReason('BadEncoding');
    expect(result).toContain('malformed');
  });

  it('maps NotValidForName to human-readable text', () => {
    const result = explainTrustReason('NotValidForName');
    expect(result).toContain('hostname');
  });

  it('returns the raw string for unknown error codes', () => {
    const raw = 'SomeUnknownError(42)';
    expect(explainTrustReason(raw)).toBe(raw);
  });

  it('matches substrings — rustls errors may have context appended', () => {
    const result = explainTrustReason('Certificate has UnknownIssuer in chain');
    expect(result).not.toBe('Certificate has UnknownIssuer in chain');
  });
});

describe('isUnknownIssuer', () => {
  it('returns true for UnknownIssuer string', () => {
    expect(isUnknownIssuer('UnknownIssuer')).toBe(true);
  });

  it('returns true when UnknownIssuer is embedded in a longer string', () => {
    expect(isUnknownIssuer('chain error: UnknownIssuer')).toBe(true);
  });

  it('returns false for other error codes', () => {
    expect(isUnknownIssuer('Expired')).toBe(false);
    expect(isUnknownIssuer('BadSignature')).toBe(false);
  });

  it('returns false for undefined', () => {
    expect(isUnknownIssuer(undefined)).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(isUnknownIssuer('')).toBe(false);
  });
});
