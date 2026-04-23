import { describe, it, expect } from 'vitest';
import type { SseEvent } from './types';

/// Mirror the parse path used inside `fetchSse` in api.ts. Tests below
/// exercise this standalone so the logic can be verified without mocking
/// `fetch` or a `ReadableStream`.
function parseSseBuffer(buffer: string): { events: SseEvent[]; remainder: string } {
  const parts = buffer.split('\n\n');
  const remainder = parts.pop() ?? '';
  const events: SseEvent[] = [];
  for (const part of parts) {
    const dataLine = part.split('\n').find((line) => line.startsWith('data:'));
    if (!dataLine) continue;
    const json = dataLine.slice(5).trim();
    if (!json) continue;
    try {
      events.push(JSON.parse(json) as SseEvent);
    } catch {
      // malformed — skipped, matching api.ts behaviour
    }
  }
  return { events, remainder };
}

describe('SSE buffer parsing', () => {
  it('parses a category event', () => {
    const buf = 'data: {"type":"category","category":"spf","verdict":"pass","title":"SPF","detail":"ok","sub_checks":[]}\n\n';
    const { events, remainder } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    expect(remainder).toBe('');
    const [ev] = events;
    expect(ev.type).toBe('category');
    if (ev.type !== 'category') throw new Error('unreachable');
    expect(ev.category).toBe('spf');
    expect(ev.verdict).toBe('pass');
  });

  it('parses a summary event', () => {
    const buf =
      'data: {"type":"summary","grade":"A","verdicts":{"spf":"pass"},"duration_ms":42}\n\n';
    const { events } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    const [ev] = events;
    if (ev.type !== 'summary') throw new Error('unreachable');
    expect(ev.grade).toBe('A');
    expect(ev.duration_ms).toBe(42);
  });

  it('parses a summary event with skipped grade', () => {
    const buf = 'data: {"type":"summary","grade":"skipped","verdicts":{}}\n\n';
    const { events } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    const [ev] = events;
    if (ev.type !== 'summary') throw new Error('unreachable');
    expect(ev.grade).toBe('skipped');
  });

  it('parses an error event', () => {
    const buf =
      'data: {"type":"error","code":"TOO_MANY_INSPECTIONS","message":"try later"}\n\n';
    const { events } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    const [ev] = events;
    if (ev.type !== 'error') throw new Error('unreachable');
    expect(ev.code).toBe('TOO_MANY_INSPECTIONS');
    expect(ev.message).toBe('try later');
  });

  it('handles multiple events in one buffer', () => {
    const buf =
      'data: {"type":"category","category":"mx","verdict":"pass","title":"MX","detail":"","sub_checks":[]}\n\n' +
      'data: {"type":"category","category":"spf","verdict":"fail","title":"SPF","detail":"","sub_checks":[]}\n\n' +
      'data: {"type":"summary","grade":"D","verdicts":{}}\n\n';
    const { events, remainder } = parseSseBuffer(buf);
    expect(events).toHaveLength(3);
    expect(remainder).toBe('');
    expect(events[0].type).toBe('category');
    expect(events[1].type).toBe('category');
    expect(events[2].type).toBe('summary');
  });

  it('preserves an unterminated trailing event in the remainder', () => {
    const buf =
      'data: {"type":"category","category":"mx","verdict":"pass","title":"MX","detail":"","sub_checks":[]}\n\n' +
      'data: {"type":"cate';
    const { events, remainder } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    expect(remainder.startsWith('data: {"type":"cate')).toBe(true);
  });

  it('ignores SSE frames that lack a data: line', () => {
    const buf = 'event: foo\nid: 1\n\ndata: {"type":"summary","grade":"B","verdicts":{}}\n\n';
    const { events } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    const [ev] = events;
    if (ev.type !== 'summary') throw new Error('unreachable');
    expect(ev.grade).toBe('B');
  });

  it('silently drops malformed JSON rather than throwing', () => {
    const buf =
      'data: not-json\n\n' +
      'data: {"type":"summary","grade":"C","verdicts":{}}\n\n';
    const { events } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    const [ev] = events;
    if (ev.type !== 'summary') throw new Error('unreachable');
    expect(ev.grade).toBe('C');
  });
});
