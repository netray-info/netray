export interface ServerOption {
  label: string;
  detail: string;
}

const BASE_SERVERS: ServerOption[] = [
  { label: '@cloudflare', detail: '1.1.1.1 / 1.0.0.1' },
  { label: '@google', detail: '8.8.8.8 / 8.8.4.4' },
  { label: '@quad9', detail: '9.9.9.9' },
  { label: '@mullvad', detail: 'Mullvad DNS' },
  { label: '@wikimedia', detail: 'Wikimedia DNS' },
  { label: '@dns4eu', detail: 'DNS4EU' },
];

/** `allowSystem` undefined means the server has not answered or the request failed: `@system` stays hidden. */
export function serverSuggestions(allowSystem: boolean | undefined): ServerOption[] {
  const list = [...BASE_SERVERS];
  if (allowSystem === true) {
    list.push({ label: '@system', detail: 'System resolvers (/etc/resolv.conf)' });
  }
  return list;
}

/** `allowSystem` undefined means the server has not answered or the request failed: `@system` stays hidden. */
export function helpServerRows(allowSystem: boolean | undefined): { token: string; desc: string }[] {
  const rows = BASE_SERVERS.map((s) => ({ token: s.label, desc: s.detail }));
  if (allowSystem === true) {
    rows.push({ token: '@system', desc: '/etc/resolv.conf' });
  }
  rows.push({ token: '@1.2.3.4', desc: 'Custom IP (if enabled by operator)' });
  return rows;
}

export function systemResolversAllowed(cfg: unknown): boolean {
  return (
    typeof cfg === 'object' &&
    cfg !== null &&
    (cfg as { allow_system_resolvers?: unknown }).allow_system_resolvers === true
  );
}
