import type { InspectResponse, MetaResponse, ErrorInfo } from './types';

const BASE = '';

export async function inspect(input: string): Promise<InspectResponse> {
  const res = await fetch(`${BASE}/api/inspect?h=${encodeURIComponent(input)}`);
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    const err = body?.error as ErrorInfo | undefined;
    throw new Error(err?.message ?? `HTTP ${res.status}`);
  }
  return res.json();
}

export async function fetchMeta(): Promise<MetaResponse | null> {
  try {
    const res = await fetch(`${BASE}/api/meta`);
    if (!res.ok) return null;
    return res.json();
  } catch {
    return null;
  }
}
