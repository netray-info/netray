import Modal from '@netray-info/common-frontend/components/Modal';
import { MAX_SELECTORS } from '../lib/parse';

interface HelpModalProps {
  open: boolean;
  onClose: () => void;
}

export default function HelpModal(props: HelpModalProps) {
  return (
    <Modal open={props.open} onClose={props.onClose} title="Help">
      <div class="help-section">
        <div class="help-section__title">About</div>
        <p class="help-desc">
          beacon checks email security posture via DNS only — no mail servers contacted.
          Inspects MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, and
          DNSBL.{' '}
          <a href="https://netray.info/guide/email-auth.html" target="_blank" rel="noopener noreferrer">
            Email auth guide ↗
          </a>
        </p>
      </div>

      <div class="help-section">
        <div class="help-section__title">Query syntax</div>
        <code class="help-syntax">domain [dkim-selector ...]</code>
        <p class="help-desc">
          The first token is the domain (must contain a dot). Any additional whitespace-separated
          tokens are treated as DKIM selectors to probe. Up to {MAX_SELECTORS} selectors.
        </p>
        <p class="help-desc">
          Examples:{' '}
          <code class="help-syntax">example.com</code>,{' '}
          <code class="help-syntax">example.com google</code>,{' '}
          <code class="help-syntax">example.com google s1 s2</code>
        </p>
        <p class="help-desc">
          beacon auto-probes Google, Outlook, SES, Proofpoint, Mimecast and{' '}
          <code class="help-syntax">default</code>. Add selectors only for other providers —
          use the value after <code class="help-syntax">s=</code> in a{' '}
          <code class="help-syntax">DKIM-Signature</code> header.
        </p>
      </div>

      <div class="help-section">
        <div class="help-section__title">Keyboard shortcuts</div>
        <table class="shortcuts-table">
          <thead>
            <tr><th>Key</th><th>Action</th></tr>
          </thead>
          <tbody>
            <tr><td class="shortcut-key">/</td><td>Focus query input</td></tr>
            <tr><td class="shortcut-key">Enter</td><td>Submit query (when input focused)</td></tr>
            <tr><td class="shortcut-key">r</td><td>Re-run last inspected query (ignores unsaved input)</td></tr>
            <tr><td class="shortcut-key">e</td><td>Toggle explanations</td></tr>
            <tr><td class="shortcut-key">j / k</td><td>Navigate result categories</td></tr>
            <tr><td class="shortcut-key">Enter</td><td>Expand / collapse active category</td></tr>
            <tr><td class="shortcut-key">Escape</td><td>Close help / dismiss history</td></tr>
            <tr><td class="shortcut-key">?</td><td>Toggle this help</td></tr>
          </tbody>
        </table>
      </div>
    </Modal>
  );
}
