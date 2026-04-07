import type { DnsEvent, TlsEvent, IpEvent, SummaryEvent, DoneEvent } from './types';

export interface SseCallbacks {
  onDns: (data: DnsEvent) => void;
  onTls: (data: TlsEvent) => void;
  onIp: (data: IpEvent) => void;
  onSummary: (data: SummaryEvent) => void;
  onDone: (data: DoneEvent) => void;
  onError: (error: string) => void;
}

export function startCheck(domain: string, callbacks: SseCallbacks): () => void {
  const es = new EventSource(`/api/check/${encodeURIComponent(domain)}`);

  es.addEventListener('dns', (e: MessageEvent) => {
    try {
      callbacks.onDns(JSON.parse(e.data) as DnsEvent);
    } catch {
      callbacks.onError('Failed to parse dns event');
    }
  });

  es.addEventListener('tls', (e: MessageEvent) => {
    try {
      callbacks.onTls(JSON.parse(e.data) as TlsEvent);
    } catch {
      callbacks.onError('Failed to parse tls event');
    }
  });

  es.addEventListener('ip', (e: MessageEvent) => {
    try {
      callbacks.onIp(JSON.parse(e.data) as IpEvent);
    } catch {
      callbacks.onError('Failed to parse ip event');
    }
  });

  es.addEventListener('summary', (e: MessageEvent) => {
    try {
      callbacks.onSummary(JSON.parse(e.data) as SummaryEvent);
    } catch {
      callbacks.onError('Failed to parse summary event');
    }
  });

  es.addEventListener('done', (e: MessageEvent) => {
    try {
      callbacks.onDone(JSON.parse(e.data) as DoneEvent);
    } catch {
      callbacks.onError('Failed to parse done event');
    }
    es.close();
  });

  es.addEventListener('error', (e: MessageEvent) => {
    try {
      const parsed = JSON.parse(e.data) as { message?: string };
      callbacks.onError(parsed.message ?? 'Unknown error');
    } catch {
      callbacks.onError('Stream error');
    }
    es.close();
  });

  es.onerror = () => {
    if (es.readyState === EventSource.CLOSED) return;
    callbacks.onError('Connection lost');
    es.close();
  };

  return () => es.close();
}
