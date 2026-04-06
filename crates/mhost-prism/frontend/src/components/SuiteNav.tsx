const LINKS = [
  { href: 'https://netray.info', label: 'netray.info' },
  { href: 'https://ip.netray.info', label: 'IP', key: 'ip' },
  { href: 'https://dns.netray.info', label: 'DNS', key: 'dns' },
  { href: 'https://tls.netray.info', label: 'TLS', key: 'tls' },
];

interface SuiteNavProps {
  current: 'ip' | 'dns' | 'tls';
}

export function SuiteNav(props: SuiteNavProps) {
  return (
    <>
      <style>{`
        .suite-nav { display: flex; align-items: center; gap: 0; background: var(--bg-secondary); border-bottom: 1px solid var(--border); padding: 0 1rem; font-family: var(--font-mono); font-size: 0.8rem; }
        .suite-nav-home { color: var(--text-muted); text-decoration: none; padding: 0.6rem 0.75rem; }
        .suite-nav-home:hover { color: var(--text-primary); }
        .suite-nav-sep { color: var(--border); padding: 0 0.25rem; user-select: none; }
        .suite-nav-link { color: var(--text-secondary); text-decoration: none; padding: 0.6rem 0.75rem; border-bottom: 2px solid transparent; transition: color 0.15s; }
        .suite-nav-link:hover { color: var(--text-primary); }
        .suite-nav-link.active { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
        @media (max-width: 400px) { .suite-nav { font-size: 0.72rem; } .suite-nav-home, .suite-nav-link { padding: 0.5rem 0.5rem; } }
      `}</style>
      <nav class="suite-nav" aria-label="netray.info suite">
        <a href="https://netray.info" class="suite-nav-home">netray.info</a>
        <span class="suite-nav-sep" aria-hidden="true">|</span>
        {LINKS.slice(1).map(l => (
          <a href={l.href} class={`suite-nav-link${l.key === props.current ? ' active' : ''}`}>{l.label}</a>
        ))}
      </nav>
    </>
  );
}
