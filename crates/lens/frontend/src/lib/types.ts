export type Verdict = 'pass' | 'warn' | 'fail' | 'error' | 'skip';

export interface CheckItem {
  name: string;
  verdict: Verdict;
  messages?: string[];
  guide_url?: string;
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

export interface SummaryEvent {
  sections: Record<string, Verdict>;
  section_grades: Record<string, string>;
  overall: Verdict;
  grade: string;
  score: number;
  hard_fail: boolean;
  hard_fail_checks: string[];
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

export interface MetaResponse {
  site_name: string;
  version: string;
  ecosystem?: MetaEcosystem;
  profile?: ProfileData;
}
