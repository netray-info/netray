export type CheckStatus = 'pass' | 'skip' | 'warn' | 'fail';

export interface QualityCheck {
  name: string;
  label: string;
  status: CheckStatus;
  message?: string;
  explanation?: string;
}

export interface QualityReport {
  verdict: CheckStatus;
  checks: QualityCheck[];
}

export interface RedirectHop {
  url: string;
  status: number;
  location?: string;
  http_version: string;
}

export interface HttpUpgrade {
  redirects_to_https: boolean;
  status_code?: number;
  same_host: boolean;
  message: string;
  redirects: RedirectHop[];
}

export interface HstsCheck {
  status: CheckStatus;
  max_age?: number;
  include_sub_domains: boolean;
  preload: boolean;
}

export interface HeaderCheck {
  status: CheckStatus;
  value?: string;
  message?: string;
}

export interface CspReport {
  status: CheckStatus;
  enforced: boolean;
  report_only: boolean;
  directives: Record<string, string[]>;
  issues: string[];
}

export interface SecurityReport {
  hsts: HstsCheck;
  csp: CspReport;
  x_frame_options: HeaderCheck;
  permissions_policy: HeaderCheck;
  x_content_type_options: HeaderCheck;
  referrer_policy: HeaderCheck;
  coop: HeaderCheck;
  coep: HeaderCheck;
  corp: HeaderCheck;
}

export interface CorsReport {
  allows_any_origin: boolean;
  reflects_origin: boolean;
  allows_credentials: boolean;
  status: CheckStatus;
  message: string;
}

export interface CookieEntry {
  name: string;
  secure: boolean;
  httponly: boolean;
  samesite?: string;
  path?: string;
  domain?: string;
  expires?: string;
}

export interface CachingReport {
  cache_control?: string;
  directives: {
    public: boolean;
    private: boolean;
    max_age?: number;
    no_store: boolean;
    no_cache: boolean;
    must_revalidate: boolean;
    immutable: boolean;
  };
  etag: boolean;
  last_modified: boolean;
  vary: string[];
  age?: number;
}

export interface CdnReport {
  detected?: string;
  cache_status?: string;
  indicators: string[];
}

export interface FingerprintReport {
  server?: string;
  info_leakage: {
    status: CheckStatus;
    exposed_headers: string[];
  };
}

export interface ReportingReport {
  report_to: boolean;
  nel: boolean;
  csp_reporting: boolean;
}

export interface EnrichmentInfo {
  ip: string;
  org?: string;
  detail_url: string;
  ip_type?: string;
  threat?: string;
}

export interface InspectResponse {
  url: string;
  final_url: string;
  timestamp: string;
  duration_ms: number;
  http_version: string;
  alt_svc?: string;
  status: number;
  redirects: RedirectHop[];
  http_upgrade?: HttpUpgrade;
  headers: Record<string, string>;
  security: SecurityReport;
  cors: CorsReport;
  cookies: CookieEntry[];
  compression?: string;
  caching: CachingReport;
  cdn: CdnReport;
  fingerprint: FingerprintReport;
  deprecated_headers: string[];
  reporting: ReportingReport;
  quality: QualityReport;
  enrichment: EnrichmentInfo;
}
