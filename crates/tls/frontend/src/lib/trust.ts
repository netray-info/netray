/** Map raw rustls error strings to human-readable explanations. */
const REASON_MAP: [RegExp, string][] = [
  [/UnknownIssuer/, 'The issuing CA is not in the trust store. The server may be missing an intermediate certificate, or using a private/self-signed CA.'],
  [/BadSignature/, 'A certificate in the chain has an invalid signature — it was not correctly signed by its claimed issuer.'],
  [/Expired/, 'One or more certificates in the chain have expired.'],
  [/NotValidYet/, 'A certificate in the chain is not yet valid — its start date is in the future.'],
  [/Revoked/, 'A certificate in the chain has been revoked by its issuing CA.'],
  [/BadEncoding/, 'A certificate in the chain is malformed or incorrectly encoded.'],
  [/UnhandledCriticalExtension/, 'A certificate contains a critical extension that could not be processed.'],
  [/NotValidForName/, 'The certificate is not valid for the requested hostname.'],
];

export function explainTrustReason(raw: string): string {
  for (const [pattern, explanation] of REASON_MAP) {
    if (pattern.test(raw)) return explanation;
  }
  return raw;
}

/** Check if the trust failure is specifically about an unknown issuer. */
export function isUnknownIssuer(reason?: string): boolean {
  return !!reason && /UnknownIssuer/.test(reason);
}
