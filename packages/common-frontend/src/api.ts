/**
 * Fetch with an AbortController-based timeout.
 * Rejects with an AbortError if the request takes longer than timeoutMs.
 */
export async function fetchWithTimeout(
  url: string,
  init?: RequestInit,
  timeoutMs = 5000,
): Promise<Response> {
  const controller = new AbortController();
  const id = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(id);
  }
}
