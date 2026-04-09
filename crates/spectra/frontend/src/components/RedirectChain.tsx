import { Show, For } from 'solid-js';
import type { RedirectHop, HttpUpgrade } from '../lib/types';

interface Props {
  redirects: RedirectHop[];
  finalUrl: string;
  httpUpgrade?: HttpUpgrade;
}

export default function RedirectChain(props: Props) {
  return (
    <Show when={props.redirects.length > 0 || props.httpUpgrade}>
      <div class="section">
        <div class="section__title">Redirects</div>

        <Show when={props.redirects.length > 0}>
          <ul class="redirect-chain">
            <For each={props.redirects}>
              {(hop) => (
                <li class="redirect-chain__hop">
                  <span class="redirect-chain__status">{hop.status}</span>
                  <span class="redirect-chain__url">{hop.url}</span>
                  <Show when={hop.location}>
                    <span class="redirect-chain__arrow">&rarr;</span>
                    <span class="redirect-chain__url">{hop.location}</span>
                  </Show>
                  <span class="badge badge--skip">{hop.http_version}</span>
                </li>
              )}
            </For>
            <li class="redirect-chain__hop">
              <span class="redirect-chain__status">200</span>
              <span class="redirect-chain__url">{props.finalUrl}</span>
            </li>
          </ul>
        </Show>

        <Show when={props.httpUpgrade}>
          <div style={{ 'margin-top': '1rem' }}>
            <div class="section__title">HTTP Upgrade (port 80)</div>
            <p style={{ 'font-size': '0.875rem', 'margin-bottom': '0.5rem' }}>
              {props.httpUpgrade!.message}
            </p>
            <Show when={props.httpUpgrade!.redirects.length > 0}>
              <ul class="redirect-chain">
                <For each={props.httpUpgrade!.redirects}>
                  {(hop) => (
                    <li class="redirect-chain__hop">
                      <span class="redirect-chain__status">{hop.status}</span>
                      <span class="redirect-chain__url">{hop.url}</span>
                      <Show when={hop.location}>
                        <span class="redirect-chain__arrow">&rarr;</span>
                        <span class="redirect-chain__url">{hop.location}</span>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
        </Show>
      </div>
    </Show>
  );
}
