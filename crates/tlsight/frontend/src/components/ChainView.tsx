import Explain from './Explain';
import type { CertInfo } from '../lib/types';
import { certDisplayName } from '../lib/cert';

interface Props {
  chain: CertInfo[];
  explain?: boolean;
}

export default function ChainView(props: Props) {
  return (
    <div class="chain-view">
      <h2>Certificate Chain</h2>
      <Explain when={!!props.explain}>This shows the certificate trust chain from your server's leaf certificate to the root CA. Each certificate in the chain vouches for the next. A complete chain is required for browsers to trust the connection.</Explain>
      <div class="chain-view__chain">
        {props.chain.map((cert, i) => (
          <>
            {i > 0 && <span class="chain-view__arrow">→</span>}
            <div class={`chain-view__cert chain-view__cert--${cert.position}`}>
              <div class="chain-view__position">{cert.position}</div>
              <div class="chain-view__subject">{certDisplayName(cert.subject)}</div>
              <div class="chain-view__days">{cert.days_remaining} days left</div>
              <div class="chain-view__key">{cert.key_type} {cert.key_size}</div>
            </div>
          </>
        ))}
      </div>
    </div>
  );
}
