import type { MetaResponse } from './types';

export async function fetchWithTimeout(
  url: string,
  init?: RequestInit,
  timeoutMs = 20000,
): Promise<Response> {
  const controller = new AbortController();
  const id = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(id);
  }
}

export async function fetchMeta(): Promise<MetaResponse | null> {
  try {
    const res = await fetchWithTimeout('/api/meta', undefined, 5000);
    if (!res.ok) return null;
    return res.json();
  } catch {
    return null;
  }
}
