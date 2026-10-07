// ---------------------------------------------------------------------------
// Types — mirrors the mhost-lib Serialize output
// ---------------------------------------------------------------------------

interface RecordData {
  name: string;
  type: string;
  ttl: number;
  data: Record<string, unknown>;
}

export interface ResponseResult {
  Response: {
    records: RecordData[];
    response_time: { secs: number; nanos: number };
    valid_until?: string;
  };
}

export interface NxDomainResult {
  NxDomain: {
    response_time: { secs: number; nanos: number };
  };
}

// mhost error variants: Timeout, QueryRefused, ServerFailure, NoRecordsFound,
// ResolveError { reason }, ProtoError { reason }, CancelledError, RuntimePanicError
export type LookupResult = ResponseResult | NxDomainResult | Record<string, unknown>;

export interface Lookup {
  query: {
    name: string;
    record_type: string;
  };
  name_server: string;
  result: LookupResult;
}

/** Raw batch from backend — lookups is a Lookups struct with inner lookups array. */
export interface RawBatchEvent {
  request_id?: string;
  record_type: string;
  lookups: { lookups: Lookup[] };
  completed: number;
  total: number;
  transport?: string;
  source?: string;
}

export interface BatchEvent {
  request_id?: string;
  record_type: string;
  lookups: Lookup[];
  completed: number;
  total: number;
  transport?: string;
  source?: string;
}

/** Parse the raw backend format into a flat BatchEvent. */
export function parseBatchEvent(raw: RawBatchEvent): BatchEvent {
  const ev: BatchEvent = {
    request_id: raw.request_id,
    record_type: raw.record_type,
    lookups: raw.lookups?.lookups ?? [],
    completed: raw.completed,
    total: raw.total,
  };
  if (raw.transport) ev.transport = raw.transport;
  if (raw.source) ev.source = raw.source;
  return ev;
}

export interface DoneStats {
  request_id?: string;
  total_queries: number;
  duration_ms: number;
  warnings: string[];
  transport?: string;
  dnssec?: boolean;
  cache_key?: string;
}
