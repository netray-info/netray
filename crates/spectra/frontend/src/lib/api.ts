import type { InspectResponse } from './types';

export interface MetaResponse {
  site_name?: string;
  version?: string;
  ecosystem?: {
    ip_base_url?: string;
    dns_base_url?: string;
    tls_base_url?: string;
    http_base_url?: string;
    lens_base_url?: string;
  };
}

function fetchWithTimeout(url: string, init: RequestInit = {}, timeoutMs = 5000): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  return fetch(url, { ...init, signal: controller.signal }).finally(() => clearTimeout(timer));
}

export async function inspect(url: string): Promise<InspectResponse> {
  const response = await fetchWithTimeout('/api/inspect', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url }),
  }, 35000);

  if (!response.ok) {
    const body = await response.json().catch(() => null);
    const message = body?.error?.message || `HTTP ${response.status}`;
    throw new Error(message);
  }

  return response.json();
}

export async function fetchMeta(): Promise<MetaResponse | null> {
  try {
    const res = await fetch('/api/meta');
    if (!res.ok) return null;
    return res.json();
  } catch {
    return null;
  }
}
