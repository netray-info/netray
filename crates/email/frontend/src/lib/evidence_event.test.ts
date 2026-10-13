import { describe, it, expect, vi, afterEach } from 'vitest';
import { streamInspect } from './api';
import type { Category, CheckResult, SseEvent, SummaryEvent } from './types';

// App.tsx folds events inline in `onEvent`; this mirrors that fold:
// category -> categories map + completedCount, summary -> summary.
interface AppState {
  categories: Map<Category, CheckResult>;
  completedCount: number;
  summary: SummaryEvent | null;
}

function fold(events: SseEvent[]): AppState {
  const state: AppState = { categories: new Map(), completedCount: 0, summary: null };
  for (const event of events) {
    if (event.type === 'category') {
      state.categories.set(event.category, event);
      state.completedCount += 1;
    } else if (event.type === 'summary') {
      state.summary = event;
    }
  }
  return state;
}

const category = {
  type: 'category', category: 'spf', verdict: 'pass', title: 'SPF', detail: 'ok', sub_checks: [],
};
const evidence = {
  type: 'evidence',
  blocks: [{ id: 'b1', kind: 'dns', records: [{ name: 'example.com', type: 'TXT' }] }],
};
const summary = { type: 'summary', grade: 'A', verdicts: { spf: 'pass' }, duration_ms: 42 };

function sse(messages: object[]): string {
  return messages.map((m) => `data: ${JSON.stringify(m)}\n\n`).join('');
}

// Drives the real streamInspect (POST path, selectors non-empty) over a stubbed fetch.
async function run(messages: object[]) {
  const body = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new TextEncoder().encode(sse(messages)));
      c.close();
    },
  });
  vi.stubGlobal('fetch', vi.fn(async () => new Response(body, { status: 200 })));

  const events: SseEvent[] = [];
  const errors: string[] = [];
  await new Promise<void>((resolve) => {
    streamInspect('example.com', ['sel1'], {
      onEvent: (e) => events.push(e),
      onError: (msg) => errors.push(msg),
      onDone: resolve,
    });
  });
  return { events, errors, state: fold(events) };
}

afterEach(() => vi.unstubAllGlobals());

describe('evidence SSE event (C7)', () => {
  it('leaves state and errors identical to category -> summary', async () => {
    const baseline = await run([category, summary]);
    const withEvidence = await run([category, evidence, summary]);

    expect(withEvidence.errors).toEqual([]);
    expect(withEvidence.state.summary).not.toBeNull();
    expect(withEvidence.state.summary?.grade).toBe('A');
    expect(withEvidence.state.categories.size).toBe(1);
    expect(withEvidence.state).toEqual(baseline.state);
    expect(baseline.errors).toEqual([]);
  });
});
