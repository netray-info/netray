import type { CertInfo } from '../lib/types';

interface Props {
  chain: CertInfo[];
}

export default function ChainView(props: Props) {
  return (
    <div class="chain-view">
      <h2>Certificate Chain</h2>
      <div class="chain-view__chain">
        {props.chain.map((cert, i) => (
          <>
            {i > 0 && <span class="chain-view__arrow">→</span>}
            <div class={`chain-view__cert chain-view__cert--${cert.position}`}>
              <div class="chain-view__position">{cert.position}</div>
              <div class="chain-view__subject">{cert.subject}</div>
              <div class="chain-view__days">{cert.days_remaining} days left</div>
              <div class="chain-view__key">{cert.key_type} {cert.key_size}</div>
            </div>
          </>
        ))}
      </div>
    </div>
  );
}
