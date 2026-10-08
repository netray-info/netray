export type TestEnv = 'local' | 'production';

export interface ServiceURLs {
  site: string;
  ip: string;
  dns: string;
  tls: string;
  http: string;
  email: string;
  lens: string;
}

const ENVS: Record<TestEnv, ServiceURLs> = {
  local: {
    site: 'http://localhost:8080',
    ip: 'http://localhost:8000',
    dns: 'http://localhost:8081',
    tls: 'http://localhost:8082',
    http: 'http://localhost:8083',
    email: 'http://localhost:8084',
    lens: 'http://localhost:8085',
  },
  production: {
    site: 'https://netray.info',
    ip: 'https://ip.netray.info',
    dns: 'https://dns.netray.info',
    tls: 'https://tls.netray.info',
    http: 'https://http.netray.info',
    email: 'https://email.netray.info',
    lens: 'https://lens.netray.info',
  },
};

export function resolveEnv(): ServiceURLs {
  const env = (process.env.TEST_ENV ?? 'local') as TestEnv;
  const urls = ENVS[env];
  if (!urls) {
    throw new Error(`Unknown TEST_ENV: ${env}. Expected 'local' or 'production'.`);
  }
  if (env !== 'local') return urls;
  // `just acceptance-local` starts the stack on free ports and passes them as
  // LOCAL_<NAME>_URL, so it never collides with other servers on the default ports.
  const overridden = { ...urls };
  for (const name of Object.keys(urls) as (keyof ServiceURLs)[]) {
    const url = process.env[`LOCAL_${name.toUpperCase()}_URL`];
    if (url) overridden[name] = url;
  }
  return overridden;
}

export function testEnv(): TestEnv {
  return (process.env.TEST_ENV ?? 'local') as TestEnv;
}

export function isProduction(): boolean {
  return testEnv() === 'production';
}

/** All 7 origins (static site + 6 tools) */
export function allOrigins(): { name: string; url: string }[] {
  const urls = resolveEnv();
  return [
    { name: 'site', url: urls.site },
    { name: 'ip', url: urls.ip },
    { name: 'dns', url: urls.dns },
    { name: 'tls', url: urls.tls },
    { name: 'http', url: urls.http },
    { name: 'email', url: urls.email },
    { name: 'lens', url: urls.lens },
  ];
}

/** 6 tool origins (no static site) */
export function toolOrigins(): { name: string; url: string }[] {
  const urls = resolveEnv();
  return [
    { name: 'ip', url: urls.ip },
    { name: 'dns', url: urls.dns },
    { name: 'tls', url: urls.tls },
    { name: 'http', url: urls.http },
    { name: 'email', url: urls.email },
    { name: 'lens', url: urls.lens },
  ];
}
