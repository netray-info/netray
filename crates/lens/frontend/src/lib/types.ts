export type Verdict = 'pass' | 'warn' | 'fail' | 'error' | 'skip';

export interface CheckItem {
  name: string;
  verdict: Verdict;
  message?: string;
  guide_url?: string;
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
  dns: Verdict;
  tls: Verdict;
  ip: Verdict;
  overall: Verdict;
  grade: string;
  score: number;
  hard_fail: boolean;
}

export interface DoneEvent {
  domain: string;
  duration_ms: number;
  cached: boolean;
}

export type CheckState = 'idle' | 'loading' | 'done' | 'error';
