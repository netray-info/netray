import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';

interface AppFooterProps {
  version?: string;
}

export default function AppFooter(props: AppFooterProps) {
  return (
    <SiteFooter
      aboutText={
        <>
          <em>beacon</em> performs DNS-only email security analysis. Built in{' '}
          <a href="https://www.rust-lang.org" target="_blank" rel="noopener noreferrer">Rust</a> with{' '}
          <a href="https://github.com/tokio-rs/axum" target="_blank" rel="noopener noreferrer">Axum</a> and{' '}
          <a href="https://www.solidjs.com" target="_blank" rel="noopener noreferrer">SolidJS</a>.
          Open to use — rate limiting applies. Part of the{' '}
          <a href="https://netray.info" target="_blank" rel="noopener noreferrer"><strong>netray.info</strong></a> suite.
        </>
      }
      links={[
        { href: '/docs', label: 'API Docs' },
        { href: 'https://lukas.pustina.de', label: 'Author', external: true },
      ]}
      version={props.version}
    />
  );
}
