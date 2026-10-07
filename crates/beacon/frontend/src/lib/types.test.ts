import { describe, it, expect } from 'vitest';
import type {
  CheckResult,
  ErrorEvent,
  SseEvent,
  SummaryEvent,
  Verdict,
} from './types';
import { CATEGORY_ORDER, GROUP_CATEGORIES, GROUP_ORDER, VERDICT_ORDER } from './types';

// ---- SseEvent discriminant narrowing --------------------------------------

function describeEvent(event: SseEvent): string {
  switch (event.type) {
    case 'category':
      // The category branch must expose CheckResult shape.
      return `category:${event.category}:${event.verdict}`;
    case 'summary':
      // The summary branch must expose SummaryEvent shape.
      return `summary:${event.grade}`;
    case 'error':
      // The error branch must expose ErrorEvent shape.
      return `error:${event.code}`;
  }
}

describe('SseEvent discriminant narrowing', () => {
  it('narrows to CheckResult on type === "category"', () => {
    const ev: CheckResult = {
      type: 'category',
      category: 'spf',
      verdict: 'pass',
      title: 'SPF',
      detail: 'ok',
      sub_checks: [],
    };
    expect(describeEvent(ev)).toBe('category:spf:pass');
  });

  it('narrows to SummaryEvent on type === "summary"', () => {
    const ev: SummaryEvent = {
      type: 'summary',
      grade: 'A',
      verdicts: { spf: 'pass' },
      duration_ms: 1234,
    };
    expect(describeEvent(ev)).toBe('summary:A');
  });

  it('narrows to ErrorEvent on type === "error"', () => {
    const ev: ErrorEvent = {
      type: 'error',
      code: 'TOO_MANY_INSPECTIONS',
      message: 'slow down',
    };
    expect(describeEvent(ev)).toBe('error:TOO_MANY_INSPECTIONS');
  });

  it('accepts grade "skipped" per SDD G3', () => {
    const ev: SummaryEvent = {
      type: 'summary',
      grade: 'skipped',
      verdicts: {},
    };
    expect(ev.grade).toBe('skipped');
  });
});

// ---- Verdict ordering ------------------------------------------------------

describe('VERDICT_ORDER', () => {
  it('orders skip < pass < info < warn < fail', () => {
    const order: Verdict[] = ['skip', 'pass', 'info', 'warn', 'fail'];
    for (let i = 0; i < order.length - 1; i++) {
      expect(VERDICT_ORDER[order[i]]).toBeLessThan(VERDICT_ORDER[order[i + 1]]);
    }
  });
});

// ---- Category ordering -----------------------------------------------------

describe('CATEGORY_ORDER', () => {
  it('covers every category in GROUP_CATEGORIES', () => {
    const fromGroups = GROUP_ORDER.flatMap((g) => GROUP_CATEGORIES[g]);
    expect(CATEGORY_ORDER).toEqual(fromGroups);
  });

  it('has 12 categories matching the beacon check count', () => {
    expect(CATEGORY_ORDER).toHaveLength(12);
  });
});
