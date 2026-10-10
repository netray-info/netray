import type { SseEvent } from './types';

export interface MetaResponse {
  version?: string;
  service?: string;
  ecosystem?: {
    ip_base_url?: string;
    dns_base_url?: string;
    tls_base_url?: string;
    http_base_url?: string;
    email_base_url?: string;
    lens_base_url?: string;
  };
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

export type StreamErrorKind = 'backend' | 'disconnect';

export interface StreamCallbacks {
  onEvent: (event: SseEvent) => void;
  onError: (msg: string, kind: StreamErrorKind) => void;
  onDone: () => void;
}

/**
 * Stream inspection results via SSE.
 * Uses GET /inspect/{domain} when no selectors, POST /inspect otherwise.
 */
export function streamInspect(
  domain: string,
  selectors: string[],
  cb: StreamCallbacks,
): AbortController {
  const controller = new AbortController();
  let summaryReceived = false;

  if (selectors.length === 0) {
    // GET — can use EventSource for simplicity
    const encoded = encodeURIComponent(domain);
    const es = new EventSource(`/inspect/${encoded}`);

    controller.signal.addEventListener('abort', () => es.close());

    es.onmessage = (e) => {
      try {
        const event: SseEvent = JSON.parse(e.data);
        if (event.type === 'error') {
          cb.onEvent(event);
          cb.onError(`${event.code}: ${event.message}`, 'backend');
          es.close();
          cb.onDone();
          return;
        }
        cb.onEvent(event);
        if (event.type === 'summary') {
          summaryReceived = true;
          es.close();
          cb.onDone();
        }
      } catch {
        // ignore parse errors
      }
    };

    es.onerror = () => {
      es.close();
      // If summary already arrived, this is a normal close — ignore.
      if (summaryReceived) return;
      cb.onError('Connection lost before inspection finished', 'disconnect');
      cb.onDone();
    };
  } else {
    // POST — use fetch + ReadableStream to parse SSE
    fetchSse(domain, selectors, controller.signal, cb);
  }

  return controller;
}

async function fetchSse(
  domain: string,
  selectors: string[],
  signal: AbortSignal,
  cb: StreamCallbacks,
): Promise<void> {
  let summaryReceived = false;
  try {
    const res = await fetch('/inspect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ domain, dkim_selectors: selectors }),
      signal,
    });

    if (!res.ok) {
      const body = await res.json().catch(() => null);
      const code = body?.error?.code;
      const message = body?.error?.message ?? `HTTP ${res.status}`;
      const msg = code ? `${code}: ${message}` : message;
      cb.onError(msg, 'backend');
      cb.onDone();
      return;
    }

    const reader = res.body?.getReader();
    if (!reader) {
      cb.onError('No response body', 'backend');
      cb.onDone();
      return;
    }

    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Parse SSE: split on double newlines
      const parts = buffer.split('\n\n');
      buffer = parts.pop() ?? '';

      for (const part of parts) {
        const dataLine = part
          .split('\n')
          .find((line) => line.startsWith('data:'));
        if (!dataLine) continue;
        const json = dataLine.slice(5).trim();
        if (!json) continue;
        try {
          const event: SseEvent = JSON.parse(json);
          if (event.type === 'error') {
            cb.onEvent(event);
            cb.onError(`${event.code}: ${event.message}`, 'backend');
            cb.onDone();
            return;
          }
          cb.onEvent(event);
          if (event.type === 'summary') summaryReceived = true;
        } catch {
          // skip malformed events
        }
      }
    }

    if (!summaryReceived) {
      cb.onError('Connection lost before inspection finished', 'disconnect');
    }
  } catch (e: unknown) {
    if (e instanceof DOMException && e.name === 'AbortError') return;
    const msg = e instanceof Error ? e.message : 'Request failed';
    cb.onError(msg, 'disconnect');
  }
  cb.onDone();
}
