import { For } from 'solid-js';

const EXAMPLE_DOMAINS = ['netray.info', 'gmail.com', 'example.com'];

interface IdleStateProps {
  onRunQuery: (d: string) => void;
  onSetQuery: (d: string) => void;
}

export default function IdleState(props: IdleStateProps) {
  return (
    <div class="empty-state">
      <div class="empty-state__title">Inspect any domain</div>
      <p>
        Check MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, and
        DNSBL — DNS-only, no mail servers contacted.
      </p>
      <div class="example-chips">
        <For each={EXAMPLE_DOMAINS}>
          {(d) => (
            <button
              type="button"
              class="example-chip"
              onClick={() => { props.onSetQuery(d); props.onRunQuery(d); }}
            >
              {d}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
