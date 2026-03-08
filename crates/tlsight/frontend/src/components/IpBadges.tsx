import { For } from 'solid-js';
import type { IpEnrichment } from '../lib/types';

interface Props {
  info: IpEnrichment;
}

interface Badge {
  label: string;
  cls: string;
}

function computeBadges(info: IpEnrichment): Badge[] {
  const b: Badge[] = [];
  if (info.cloud?.provider) {
    b.push({ label: info.cloud.provider, cls: 'ip-badge--cloud' });
  } else if (info.network_type) {
    b.push({ label: info.network_type, cls: 'ip-badge--type' });
  }
  if (info.is_tor) b.push({ label: 'Tor', cls: 'ip-badge--threat' });
  if (info.is_vpn) b.push({ label: 'VPN', cls: 'ip-badge--threat' });
  if (info.is_spamhaus) b.push({ label: 'Spamhaus', cls: 'ip-badge--threat' });
  if (info.is_c2) b.push({ label: 'C2', cls: 'ip-badge--threat' });
  if (info.org && b.length === 0) {
    b.push({ label: info.org, cls: 'ip-badge--org' });
  }
  return b;
}

export default function IpBadges(props: Props) {
  const badges = () => computeBadges(props.info);

  return (
    <For each={badges()}>
      {(badge) => <span class={`ip-badge ${badge.cls}`}>{badge.label}</span>}
    </For>
  );
}
