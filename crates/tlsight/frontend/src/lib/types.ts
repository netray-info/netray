export interface InspectResponse {
  request_id: string;
  hostname: string;
  input_mode: 'hostname' | 'ip';
  summary: Summary;
  ports: PortResult[];
  dns: DnsContext | null;
  warnings?: string[];
  skipped_ips?: string[];
  duration_ms: number;
}

export type CheckStatus = 'pass' | 'warn' | 'fail' | 'skip';

export interface Summary {
  verdict: CheckStatus;
  checks: {
    chain_trusted: CheckStatus;
    not_expired: CheckStatus;
    hostname_match: CheckStatus;
    caa_compliant: CheckStatus;
    dane_valid: CheckStatus;
    ct_logged: CheckStatus;
    ocsp_stapled: CheckStatus;
    consistency: CheckStatus;
  };
}

export interface PortResult {
  port: number;
  ips: IpResult[];
  consistency?: ConsistencyInfo;
  validation?: ValidationInfo;
  tlsa?: TlsaInfo;
  error?: ErrorInfo;
}

export interface IpResult {
  ip: string;
  ip_version: 'v4' | 'v6';
  tls?: TlsInfo;
  chain?: CertInfo[];
  validation?: ValidationInfo;
  ct?: CtInfo;
  enrichment?: IpEnrichment;
  error?: ErrorInfo;
}

export interface IpEnrichment {
  network_type?: string;
  asn?: number;
  org?: string;
  cloud?: { provider?: string; region?: string; service?: string };
  is_tor: boolean;
  is_vpn: boolean;
  is_datacenter: boolean;
  is_spamhaus: boolean;
  is_c2: boolean;
}

export interface CtInfo {
  sct_count: number;
  scts: SctEntry[];
}

export interface SctEntry {
  version: number;
  log_id: string;
  timestamp: string;
}

export interface TlsInfo {
  version: string;
  cipher_suite: string;
  alpn: string | null;
  sni: string | null;
  ocsp: OcspInfo;
  handshake_ms: number;
}

export interface OcspInfo {
  stapled: boolean;
  status: 'good' | 'revoked' | 'unknown' | 'malformed' | null;
  this_update: string | null;
  next_update: string | null;
}

export interface CertInfo {
  position: 'leaf' | 'intermediate' | 'root' | 'self_signed' | 'leaf_self_signed';
  subject: string;
  issuer: string;
  sans: string[];
  serial: string;
  not_before: string;
  not_after: string;
  days_remaining: number;
  key_type: string;
  key_size: number;
  signature_algorithm: string;
  fingerprint_sha256: string;
  is_expired: boolean;
  is_self_signed: boolean;
}

export interface ValidationInfo {
  chain_trusted: boolean;
  chain_trust_reason?: string;
  terminates_at_self_signed: boolean;
  chain_order_correct: boolean;
  leaf_covers_hostname: boolean;
  any_expired: boolean;
  any_not_yet_valid: boolean;
  weakest_signature: string;
  earliest_expiry: string;
  earliest_expiry_days: number;
}

export interface TlsaInfo {
  records: string[];
  dnssec_signed: boolean;
  dane_valid: boolean | null;
}

export interface ConsistencyInfo {
  certificates_match: boolean;
  tls_versions_match: boolean;
  cipher_suites_match: boolean;
  mismatches: ConsistencyMismatch[];
}

export interface ConsistencyMismatch {
  field: string;
  values: Record<string, string>;
}

export interface DnsContext {
  caa: CaaInfo | null;
  resolved_ips: string[];
}

export interface CaaInfo {
  records: string[];
  issuer_allowed: boolean | null;
  issuewild_present: boolean;
}

export interface ErrorInfo {
  code: string;
  message: string;
}

export interface MetaResponse {
  version: string;
  features: Record<string, boolean>;
  limits: Record<string, number>;
  ecosystem?: {
    dns_base_url?: string;
    ip_base_url?: string;
  };
}
