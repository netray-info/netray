import { For, Show } from 'solid-js';
import type { CheckItem, Verdict } from '../lib/types';

function verdictIcon(v: Verdict): string {
  switch (v) {
    case 'pass':  return '✓';
    case 'warn':  return '!';
    case 'fail':  return '✗';
    case 'error': return '✗';
    case 'skip':  return '–';
  }
}

function verdictColor(v: Verdict): string {
  switch (v) {
    case 'pass':  return 'var(--verdict-pass)';
    case 'warn':  return 'var(--verdict-warn)';
    case 'fail':  return 'var(--verdict-fail)';
    case 'error': return 'var(--verdict-error)';
    case 'skip':  return 'var(--verdict-skip)';
  }
}

const CHECK_DESCRIPTIONS: Record<string, string> = {
  // DNS
  spf: 'SPF authorises which servers may send email for this domain',
  dmarc: 'DMARC instructs receivers how to handle unauthenticated messages',
  dnssec: 'DNSSEC cryptographically signs DNS records, preventing cache poisoning',
  caa: 'CAA restricts which Certificate Authorities may issue certificates',
  mx: 'MX records must agree between recursive and authoritative resolvers',
  ns: 'At least two NS records ensure availability if one nameserver fails',
  ns_lame: 'All NS records should respond authoritatively for the zone',
  ns_delegation: 'Parent and child NS records must be consistent',
  dkim: 'DKIM signs outgoing mail to prove it hasn\'t been tampered with',
  mta_sts: 'MTA-STS enforces TLS for inbound SMTP connections',
  tlsrpt: 'SMTP TLS Reporting provides visibility into delivery failures',
  bimi: 'BIMI displays a brand logo in supporting email clients',
  cname_apex: 'A CNAME at the zone apex breaks MX, NS and other records',
  https_svcb: 'HTTPS DNS records enable HTTP/3 and Encrypted Client Hello',
  ttl: 'Inconsistent TTLs across record types can cause caching problems',
  dnskey_algorithm: 'Deprecated DNSSEC algorithms (RSA/MD5, RSA/SHA-1) must be replaced',
  dnssec_rollover: 'DNSSEC key rollover must be clean — no orphaned DS or duplicate KSKs',
  infrastructure: 'Infrastructure checks: NS lame delegation and delegation consistency',
  // TLS
  chain_trusted: 'The certificate chain must verify to a trusted root CA',
  not_expired: 'All certificates in the chain must be within their validity period',
  hostname_match: 'The leaf certificate SAN must cover the queried hostname',
  chain_complete: 'All intermediate certificates must be present in correct order',
  strong_signature: 'SHA-1 and MD5 signatures are deprecated and must not appear in the chain',
  key_strength: 'RSA keys must be ≥ 2048 bits; ECDSA must use P-256 or better',
  expiry_window: 'Certificate expires within 30 days (warn) or 7 days (fail)',
  cert_lifetime: 'CA/Browser Forum requires certificates to be valid for ≤ 398 days',
  san_quality: 'The SAN list should be reasonable in size and not overly broad',
  aia_reachability: 'AIA CA Issuers URL must be reachable when the chain is incomplete',
  tls_version: 'TLS 1.3 is preferred; TLS 1.2 acceptable; older versions must not be offered',
  forward_secrecy: 'Ephemeral key exchange protects past sessions if the key is later compromised',
  aead_cipher: 'AEAD ciphers (GCM, ChaCha20-Poly1305) provide authenticated encryption',
  ocsp_stapled: 'OCSP stapling avoids a separate revocation check by the browser',
  ct_logged: 'Certificate Transparency requires ≥ 2 SCTs for browser trust',
  caa_compliant: 'The issuing CA must be authorised by the domain\'s CAA records',
  dane_valid: 'TLSA records must match the presented certificate if DANE is configured',
  consistency: 'All resolved IPs must present the same certificate and TLS configuration',
  alpn_consistency: 'ALPN protocol negotiation must be consistent across all IPs',
  ech_advertised: 'Encrypted Client Hello hides the hostname from passive observers',
  hsts: 'HTTP Strict Transport Security forces browsers to use HTTPS for ≥ 6 months',
  https_redirect: 'HTTP must redirect to HTTPS',
  // IP
  reputation: 'IP reputation: VPNs warn, Tor exit nodes and known C2 hosts fail',
};

interface Props {
  checks: CheckItem[];
  explain: boolean;
}

export default function CheckList(props: Props) {
  return (
    <ul class="check-list" role="list">
      <For each={props.checks}>
        {(check) => (
          <li class="check-item">
            <span
              class="verdict-icon"
              style={{ color: verdictColor(check.verdict) }}
              aria-label={check.verdict}
            >
              {verdictIcon(check.verdict)}
            </span>
            <span class="check-item__name">
              {check.name}
              <Show when={props.explain && CHECK_DESCRIPTIONS[check.name] !== undefined}>
                <span class="check-item__desc">{CHECK_DESCRIPTIONS[check.name]}</span>
              </Show>
            </span>
            {check.message && (
              <span class="check-item__message">{check.message}</span>
            )}
            <Show when={check.guide_url}>
              <a
                class="check-item__guide-link"
                href={check.guide_url}
                target="_blank"
                rel="noopener noreferrer"
              >
                Learn why
              </a>
            </Show>
          </li>
        )}
      </For>
    </ul>
  );
}
