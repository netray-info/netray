import type { InspectResponse, MetaResponse, ErrorInfo } from './types';
import { fetchWithTimeout } from '@netray-info/common-frontend/api';

const BASE = '';

export async function inspect(input: string): Promise<InspectResponse> {
  const url = `${BASE}/api/inspect?h=${encodeURIComponent(input)}`;
  const res = await fetch(url);
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    const err = body?.error as ErrorInfo | undefined;
    throw new Error(err?.message ?? `HTTP ${res.status}`);
  }
  return res.json();
}

export async function fetchMeta(): Promise<MetaResponse | null> {
  try {
    const res = await fetchWithTimeout(`${BASE}/api/meta`, undefined, 5000);
    if (!res.ok) return null;
    return res.json();
  } catch {
    return null;
  }
}
