import { createSignal, createEffect, Show, For } from 'solid-js';
import Explain from './Explain';
import type { Summary, QualityResult, PortQualityResult, HealthCheck, CheckStatus, QualityCategory } from '../lib/types';

interface Props {
  summary: Summary;
  quality?: QualityResult;
  portQualities: { port: number; quality?: PortQualityResult }[];
  explain?: boolean;
  expanded?: boolean;
}

const CHECK_LABELS: Record<string, string> = {
  chain_trusted: 'Chain trusted',
  not_expired: 'Not expired',
  hostname_match: 'Hostname match',
  caa_compliant: 'CAA compliant',
  dane_valid: 'DANE valid',
  ct_logged: 'CT logged',
  ocsp_stapled: 'OCSP stapled',
  consistency: 'Consistency',
};

const CHECK_EXPLANATIONS: Record<string, string> = {
  chain_trusted: 'The certificate chain is complete and all signatures verify up to a trusted root CA.',
  not_expired: 'No certificate in the chain has expired or is not yet valid.',
  hostname_match: 'The leaf certificate\'s SANs (Subject Alternative Names) include the queried hostname.',
  caa_compliant: 'The issuing CA is authorized by the domain\'s CAA DNS records (or no CAA records exist).',
  dane_valid: 'TLSA records in DNS match the presented certificate (requires DNSSEC).',
  ct_logged: 'The certificate has Signed Certificate Timestamps (SCTs) proving it was logged in CT logs.',
  ocsp_stapled: 'The server includes an OCSP response, allowing clients to check revocation without contacting the CA.',
  consistency: 'All IP addresses for this hostname serve the same certificate and TLS configuration.',
  alpn_consistency: 'When multiple IPs are inspected, checks that all negotiate the same ALPN protocol (e.g. h2 vs http/1.1). Divergence means some IPs offer HTTP/2 and others do not.',
};

const STATUS_ICON: Record<CheckStatus, string> = {
  pass: '\u2713',
  warn: '\u26A0',
  fail: '\u2717',
  skip: '\u2014',
};

const CATEGORY_LABELS: Record<QualityCategory, string> = {
  certificate: 'Certificate',
  protocol: 'Protocol',
  configuration: 'Configuration',
};

const DETAIL_EXPLANATIONS: Record<string, string> = {
  chain_trusted: 'Verifies the certificate chain terminates at a trusted root CA in the Mozilla trust store.',
  not_expired: 'Checks that no certificate in the chain has expired or is not yet valid.',
  hostname_match: 'Confirms the leaf certificate\'s SANs include the queried hostname.',
  chain_complete: 'Verifies the chain is correctly ordered — each certificate\'s issuer matches the next certificate\'s subject.',
  strong_signature: 'Checks that no certificate uses a weak signature algorithm (SHA-1 or MD5).',
  key_strength: 'Verifies RSA keys are at least 2048 bits. ECDSA and Ed25519 keys always pass.',
  expiry_window: 'Warns when any certificate expires within 30 days, fails within 7 days.',
  cert_lifetime: 'Checks the leaf certificate validity period against CA/B Forum limits: warns above 398 days, fails above 825 days. Browsers enforce these limits independently of the stated expiry.',
  tls_version: 'Warns for TLS 1.2 (deprecated, prefer TLS 1.3). Fails for TLS 1.0, 1.1, and SSLv3 (insecure).',
  forward_secrecy: 'Checks the cipher suite uses ECDHE or DHE key exchange. Static RSA key exchange does not provide forward secrecy.',
  aead_cipher: 'Checks the cipher suite uses an AEAD mode (GCM, ChaCha20-Poly1305, CCM). CBC mode ciphers are vulnerable to padding oracle attacks.',
  ct_logged: 'Checks for Signed Certificate Timestamps (SCTs) proving the certificate was logged in Certificate Transparency logs. Chrome requires 2+ SCTs.',
  ocsp_stapled: 'Checks that the server staples an OCSP response so clients can verify revocation without contacting the CA.',
  caa_compliant: 'Verifies the issuing CA is authorized by the domain\'s CAA DNS records.',
  dane_valid: 'Checks that TLSA records in DNS match the presented certificate (requires DNSSEC).',
  ech_advertised: 'Checks whether Encrypted Client Hello (ECH) is advertised in the HTTPS DNS record. ECH prevents the SNI from being visible to passive observers. Only checked on port 443.',
  consistency: 'When multiple IPs are inspected, checks that all serve the same certificate and TLS configuration.',
  alpn_consistency: 'When multiple IPs are inspected, checks that all negotiate the same ALPN protocol (e.g. h2 vs http/1.1). Divergence means some IPs offer HTTP/2 and others do not.',
  hsts: 'HTTP Strict Transport Security tells browsers to always use HTTPS. A max-age of at least 6 months (15768000s) is recommended.',
  https_redirect: 'Checks whether port 80 redirects HTTP requests to HTTPS on the same host.',
};

function countStatuses(checks: Record<string, CheckStatus>): Record<CheckStatus, number> {
  const counts: Record<CheckStatus, number> = { pass: 0, warn: 0, fail: 0, skip: 0 };
  for (const status of Object.values(checks)) counts[status]++;
  return counts;
}

function groupByCategory(checks: HealthCheck[]): [string, HealthCheck[]][] {
  const groups: Record<string, HealthCheck[]> = {};
  const order: string[] = [];
  for (const c of checks) {
    if (!groups[c.category]) {
      groups[c.category] = [];
      order.push(c.category);
    }
    groups[c.category].push(c);
  }
  return order.map(cat => [cat, groups[cat]]);
}

function allQualityChecks(quality?: QualityResult, portQualities?: { port: number; quality?: PortQualityResult }[]): HealthCheck[] {
  const checks: HealthCheck[] = [];
  if (quality) checks.push(...quality.checks);
  if (portQualities) {
    for (const pq of portQualities) {
      if (pq.quality) checks.push(...pq.quality.checks);
    }
  }
  return checks;
}

function qualityCounts(quality?: QualityResult, portQualities?: { port: number; quality?: PortQualityResult }[]): Record<string, number> {
  const counts: Record<string, number> = { pass: 0, warn: 0, fail: 0, skip: 0 };
  for (const c of allQualityChecks(quality, portQualities)) counts[c.status]++;
  return counts;
}

function qualityVerdict(quality?: QualityResult, portQualities?: { port: number; quality?: PortQualityResult }[]): CheckStatus {
  const checks = allQualityChecks(quality, portQualities);
  if (checks.some(c => c.status === 'fail')) return 'fail';
  if (checks.some(c => c.status === 'warn')) return 'warn';
  if (checks.length > 0) return 'pass';
  return 'skip';
}

function CheckRow(props: { check: HealthCheck }) {
  return (
    <div class={`quality-check quality-check--${props.check.status}`} title={DETAIL_EXPLANATIONS[props.check.id]}>
      <span class="quality-check__icon" aria-hidden="true">{STATUS_ICON[props.check.status]}</span>
      <span class="sr-only">{props.check.status}</span>
      <span class="quality-check__label">{props.check.label}</span>
      <span class="quality-check__sep">&mdash;</span>
      <span class="quality-check__detail">{props.check.detail}</span>
    </div>
  );
}

function CheckGroup(props: { category: string; checks: HealthCheck[] }) {
  return (
    <div class="quality-group">
      <div class="quality-group__title">{CATEGORY_LABELS[props.category as QualityCategory] ?? props.category}</div>
      <For each={props.checks}>
        {(check) => <CheckRow check={check} />}
      </For>
    </div>
  );
}

export default function ValidationSummary(props: Props) {
  const hasQuality = () => props.quality || props.portQualities.some(p => p.quality);
  const [expanded, setExpanded] = createSignal(false);
  createEffect(() => { if (props.expanded !== undefined) setExpanded(props.expanded); });

  // Use quality verdict/counts when available, else fall back to summary
  const verdict = () => hasQuality() ? qualityVerdict(props.quality, props.portQualities) : props.summary.verdict;
  const counts = () => hasQuality() ? qualityCounts(props.quality, props.portQualities) : countStatuses(props.summary.checks);

  return (
    <div class="validation-summary" data-card>
      <button class="dns-section__toggle" onClick={() => setExpanded(!expanded())} aria-expanded={expanded() ? "true" : "false"}>
        <span class="dns-section__toggle-left">
          Validation
          <span class={`badge badge--${verdict()}`}>{verdict()}</span>
          <Show when={counts().pass > 0}>
            <span class="quality-summary-count quality-summary-count--pass">{counts().pass} passed</span>
          </Show>
          <Show when={counts().warn > 0}>
            <span class="quality-summary-count quality-summary-count--warn">{counts().warn} warning{counts().warn !== 1 ? 's' : ''}</span>
          </Show>
          <Show when={counts().fail > 0}>
            <span class="quality-summary-count quality-summary-count--fail">{counts().fail} failed</span>
          </Show>
        </span>
        <span class="ip-card__chevron" classList={{ 'ip-card__chevron--open': expanded() }}>
          &#x25B8;
        </span>
      </button>

      <div class="validation-summary__checks">
        {Object.entries(props.summary.checks).map(([key, status]) => {
          const conditional = ['dane_valid', 'ct_logged', 'ocsp_stapled', 'consistency'];
          if (conditional.includes(key) && status === 'skip') return null;
          return (
            <span class={`check check--${status}`} title={CHECK_EXPLANATIONS[key]}>
              <span aria-hidden="true">{STATUS_ICON[status]}</span>
              <span class="sr-only">{status}</span>
              {' '}{CHECK_LABELS[key] ?? key}
            </span>
          );
        })}
      </div>

      <Explain when={!!props.explain}>This is the overall validation summary. Green = good, orange = warning, red = problem, grey = skipped.</Explain>

      <Show when={expanded()}>
        <div class="quality-section__body">
          {/* Hostname-scoped checks (HSTS, redirect) */}
          <Show when={props.quality && props.quality!.checks.length > 0}>
            <div class="quality-hostname">
              <For each={props.quality!.checks}>
                {(check) => <CheckRow check={check} />}
              </For>
            </div>
          </Show>

          {/* Per-port checks */}
          <For each={props.portQualities}>
            {(pq) => (
              <Show when={pq.quality}>
                <div class="quality-port">
                  <Show when={props.portQualities.length > 1}>
                    <div class="quality-port__header">
                      <span class="quality-port__label">Port {pq.port}</span>
                      <span class={`verdict verdict--${pq.quality!.verdict}`}>{pq.quality!.verdict}</span>
                    </div>
                  </Show>
                  <For each={groupByCategory(pq.quality!.checks)}>
                    {([cat, checks]) => <CheckGroup category={cat} checks={checks} />}
                  </For>
                </div>
              </Show>
            )}
          </For>

          <Explain when={!!props.explain}>
            <dl class="check-explanations">
              <For each={allQualityChecks(props.quality, props.portQualities).filter((c, i, arr) => arr.findIndex(x => x.id === c.id) === i)}>
                {(check) => (
                  <Show when={DETAIL_EXPLANATIONS[check.id]}>
                    <>
                      <dt>{check.label}</dt>
                      <dd>{DETAIL_EXPLANATIONS[check.id]}</dd>
                    </>
                  </Show>
                )}
              </For>
            </dl>
          </Explain>
        </div>
      </Show>
    </div>
  );
}
