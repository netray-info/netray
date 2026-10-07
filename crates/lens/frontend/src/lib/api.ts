import type { MetaResponse } from './types';
import { fetchWithTimeout } from '@netray-info/common-frontend/api';

export { fetchWithTimeout };

export async function fetchMeta(): Promise<MetaResponse | null> {
  try {
    const res = await fetchWithTimeout('/api/meta', undefined, 5000);
    if (!res.ok) return null;
    return res.json();
  } catch {
    return null;
  }
}
