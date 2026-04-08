import { Show, For } from 'solid-js';
import Explain from './Explain';
import type { CertInfo, ValidationInfo } from '../lib/types';
import { certDisplayName } from '../lib/cert';
import { isUnknownIssuer } from '../lib/trust';

interface Props {
  chain: CertInfo[];
  explain?: boolean;
  validation?: ValidationInfo;
}

/** Check if cert[i].issuer matches cert[i+1].subject (chain link is valid). */
function isLinkBroken(chain: CertInfo[], i: number): boolean {
  if (i + 1 >= chain.length) return false;
  return chain[i].issuer !== chain[i + 1].subject;
}

export default function ChainView(props: Props) {
  const lastCert = () => props.chain[props.chain.length - 1];
  const needsInferredRoot = () => {
    const last = lastCert();
    return last && !last.is_self_signed;
  };
  const isRootLinkBroken = () => {
    const v = props.validation;
    return v && !v.chain_trusted && !v.terminates_at_self_signed;
  };
  /** The last presented cert is the problematic one when issuer is unknown. */
  const isUnknownIssuerCert = (i: number) => {
    const v = props.validation;
    return v && !v.chain_trusted
      && isUnknownIssuer(v.chain_trust_reason)
      && i === props.chain.length - 1;
  };

  return (
    <div class="chain-view">
      <h2>Certificate Chain</h2>
      <Explain when={!!props.explain}>This shows the certificate trust chain from your server's leaf certificate to the root CA. Each certificate in the chain vouches for the next. A complete chain is required for browsers to trust the connection. The root CA is typically not sent by the server — it lives in your browser's trust store. When omitted, it is shown here as an inferred card (dashed border) using the last certificate's issuer name.</Explain>
      <div class="chain-view__chain">
        <For each={props.chain}>
          {(cert, i) => (
            <>
              <Show when={i() > 0}>
                <span class={`chain-view__arrow${isLinkBroken(props.chain, i() - 1) ? ' chain-view__arrow--broken' : ''}`}>
                  {isLinkBroken(props.chain, i() - 1) ? <>&times;</> : <>&rarr;</>}
                </span>
              </Show>
              <div class={`chain-view__cert chain-view__cert--${cert.position}${cert.is_expired ? ' chain-view__cert--expired' : ''}${isUnknownIssuerCert(i()) ? ' chain-view__cert--unknown-issuer' : ''}`}>
                <div class="chain-view__position">{cert.position}</div>
                <div class="chain-view__subject">{certDisplayName(cert.subject)}</div>
                <div class="chain-view__days">{cert.days_remaining} days left</div>
                <div class="chain-view__key">{cert.key_type} {cert.key_size}</div>
              </div>
            </>
          )}
        </For>
        {needsInferredRoot() && (
          <>
            <span class={`chain-view__arrow${isRootLinkBroken() ? ' chain-view__arrow--broken' : ''}`}>
              {isRootLinkBroken() ? <>&times;</> : <>&rarr;</>}
            </span>
            <div class={`chain-view__cert chain-view__cert--root-inferred${props.validation && !props.validation.chain_trusted ? ' chain-view__cert--root-untrusted' : ''}`}>
              <div class="chain-view__position">root</div>
              <div class="chain-view__subject">{certDisplayName(lastCert().issuer)}</div>
              <div class="chain-view__inferred">
                {isRootLinkBroken() ? 'not in trust store' : 'from trust store'}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
