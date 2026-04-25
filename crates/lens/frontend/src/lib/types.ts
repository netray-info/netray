export type Verdict = 'pass' | 'warn' | 'fail' | 'error' | 'skip';

export interface CheckItem {
  name: string;
  verdict: Verdict;
  messages?: string[];
  guide_url?: string;
  // Plain-English remediation, populated per check by lens. Empty/missing
  // means render nothing (Phase 1 ships the mechanism; Phase 4 fills copy).
  fix_hint?: string;
  fix_owner?: string;
  weight?: number;
}

export interface DnsEvent {
  status: Verdict;
  headline: string;
  checks: CheckItem[];
  detail_url: string;
}

export interface TlsEvent {
  status: Verdict;
  headline: string;
  checks: CheckItem[];
  detail_url: string;
}

export interface HttpEvent {
  status: Verdict;
  headline: string;
  checks: CheckItem[];
  detail_url: string;
  status_code?: number;
  http_version?: string;
  response_duration_ms?: number;
  server_ip?: string;
  server_org?: string;
  server_network_type?: string;
}

export interface IpAddress {
  ip: string;
  org?: string;
  geo?: string;
  network_type: string;
}

export interface IpEvent {
  status: Verdict;
  headline: string;
  checks: CheckItem[];
  addresses: IpAddress[];
  detail_url: string;
  guide_url?: string;
}

export interface EmailEvent {
  status: Verdict;
  headline: string;
  checks: CheckItem[];
  detail_url: string;
  grade?: string;
}

export interface SummaryEvent {
  sections: Record<string, Verdict>;
  section_grades: Record<string, string>;
  overall: Verdict;
  grade: string;
  score: number;
  hard_fail: boolean;
  hard_fail_checks: string[];
  hard_fail_reason?: string;
  not_applicable: Record<string, string>;
}

export interface DoneEvent {
  domain: string;
  duration_ms: number;
  cached: boolean;
}

export type CheckState = 'idle' | 'loading' | 'done' | 'error';

export interface MetaEcosystem {
  ip_base_url?: string;
  dns_base_url?: string;
  tls_base_url?: string;
  lens_base_url?: string;
  http_base_url?: string;
}

export interface ProfileData {
  name: string;
  version: number;
  checks: Record<string, number>;
  section_weights: Record<string, number>;
  thresholds: Record<string, number>;
  hard_fail: Record<string, string[]>;
}

export interface FooterLink {
  label: string;
  href: string;
  external: boolean;
}

/// Apex-landing branding from lens [site] config. All fields optional;
/// the frontend supplies hardcoded fallbacks when meta or a single field
/// is missing.
export interface SiteMeta {
  title?: string;
  description?: string;
  og_image?: string | null;
  og_site_name?: string;
  brand_name?: string;
  brand_tagline?: string;
  status_pill?: string;
  hero_heading?: string;
  hero_subheading?: string;
  example_domains?: string[];
  trust_strip?: string;
  footer_about?: string | null;
  footer_links?: FooterLink[] | null;
}

export interface MetaFeatures {
  profile?: ProfileData;
  site?: SiteMeta;
}

export interface MetaResponse {
  site_name: string;
  version: string;
  ecosystem?: MetaEcosystem;
  features?: MetaFeatures;
}
