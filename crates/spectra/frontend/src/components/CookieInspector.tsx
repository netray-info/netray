import { Show, For } from 'solid-js';
import type { CookieEntry } from '../lib/types';

interface Props {
  cookies: CookieEntry[];
}

function BoolCell(props: { value: boolean }) {
  return (
    <td style={{ color: props.value ? 'var(--pass)' : 'var(--text-muted)' }}>
      {props.value ? 'Yes' : 'No'}
    </td>
  );
}

export default function CookieInspector(props: Props) {
  return (
    <Show when={props.cookies.length > 0}>
      <div class="section">
        <div class="section__title">Cookies ({props.cookies.length})</div>
        <table class="cookie-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Secure</th>
              <th>HttpOnly</th>
              <th>SameSite</th>
              <th>Path</th>
              <th>Domain</th>
            </tr>
          </thead>
          <tbody>
            <For each={props.cookies}>
              {(cookie) => (
                <tr>
                  <td class="mono">{cookie.name}</td>
                  <BoolCell value={cookie.secure} />
                  <BoolCell value={cookie.httponly} />
                  <td>{cookie.samesite ?? '-'}</td>
                  <td class="mono">{cookie.path ?? '-'}</td>
                  <td class="mono">{cookie.domain ?? '-'}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </Show>
  );
}
