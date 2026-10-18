export type TestEnv = 'local' | 'production';

export type OriginName = 'apex' | 'dns' | 'tls' | 'http' | 'ip';

export interface Origin {
  name: OriginName;
  url: string;
  /** Set locally, where one origin serves every name: the `Host` header that picks the name. */
  host?: string;
}

const NAMES: { name: OriginName; host: string }[] = [
  { name: 'apex', host: 'netray.info' },
  { name: 'dns', host: 'dns.netray.info' },
  { name: 'tls', host: 'tls.netray.info' },
  { name: 'http', host: 'http.netray.info' },
  { name: 'ip', host: 'ip.netray.info' },
];

export function testEnv(): TestEnv {
  const env = process.env.TEST_ENV ?? 'local';
  if (env !== 'local' && env !== 'production') {
    throw new Error(`Unknown TEST_ENV: ${env}. Expected 'local' or 'production'.`);
  }
  return env;
}

export function isProduction(): boolean {
  return testEnv() === 'production';
}

/** The apex and the four inspector hosts: five URLs in production, one local origin by `Host`. */
export function origins(): Origin[] {
  if (isProduction()) {
    return NAMES.map(({ name, host }) => ({ name, url: `https://${host}` }));
  }
  const url = process.env.LOCAL_URL ?? 'http://localhost:8082';
  return NAMES.map(({ name, host }) => ({ name, url, host }));
}

export function apex(): Origin {
  return origins()[0];
}

/** The request headers that address an origin: `Host` when one origin serves every name. */
export function hostHeaders(origin: Origin): Record<string, string> {
  return origin.host ? { Host: origin.host } : {};
}
