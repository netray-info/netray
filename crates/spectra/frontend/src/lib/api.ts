import type { InspectResponse } from './types';

export async function inspect(url: string): Promise<InspectResponse> {
  const response = await fetch('/api/inspect', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url }),
  });

  if (!response.ok) {
    const body = await response.json().catch(() => null);
    const message = body?.error?.message || `HTTP ${response.status}`;
    throw new Error(message);
  }

  return response.json();
}
