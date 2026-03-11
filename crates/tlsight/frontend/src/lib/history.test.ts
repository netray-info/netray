import { describe, it, expect, beforeEach, vi } from 'vitest';

// localStorage mock for node test environment
const store: Record<string, string> = {};
vi.stubGlobal('localStorage', {
  getItem: (key: string) => store[key] ?? null,
  setItem: (key: string, value: string) => { store[key] = value; },
  removeItem: (key: string) => { delete store[key]; },
  clear: () => { Object.keys(store).forEach(k => delete store[k]); },
  length: 0,
  key: () => null,
});

import { addToHistory, getHistory, clearHistory } from './history';

beforeEach(() => {
  localStorage.clear();
});

describe('getHistory', () => {
  it('returns empty array when storage is empty', () => {
    expect(getHistory()).toEqual([]);
  });

  it('returns parsed entries', () => {
    const entries = [{ query: 'example.com', timestamp: 1000 }];
    localStorage.setItem('tlsight_history', JSON.stringify(entries));
    expect(getHistory()).toEqual(entries);
  });

  it('returns empty array on invalid JSON', () => {
    localStorage.setItem('tlsight_history', 'not-json');
    expect(getHistory()).toEqual([]);
  });
});

describe('addToHistory', () => {
  it('adds a new entry', () => {
    addToHistory('example.com');
    const history = getHistory();
    expect(history).toHaveLength(1);
    expect(history[0].query).toBe('example.com');
    expect(typeof history[0].timestamp).toBe('number');
  });

  it('prepends to existing entries', () => {
    addToHistory('first.com');
    addToHistory('second.com');
    const history = getHistory();
    expect(history[0].query).toBe('second.com');
    expect(history[1].query).toBe('first.com');
  });

  it('deduplicates: re-adding moves entry to front', () => {
    addToHistory('first.com');
    addToHistory('second.com');
    addToHistory('first.com');
    const history = getHistory();
    expect(history[0].query).toBe('first.com');
    expect(history).toHaveLength(2);
  });

  it('caps at 20 entries', () => {
    for (let i = 0; i < 25; i++) {
      addToHistory(`host${i}.com`);
    }
    expect(getHistory()).toHaveLength(20);
  });

  it('most recent entry is always first', () => {
    for (let i = 0; i < 5; i++) {
      addToHistory(`host${i}.com`);
    }
    expect(getHistory()[0].query).toBe('host4.com');
  });
});

describe('clearHistory', () => {
  it('removes all entries', () => {
    addToHistory('example.com');
    clearHistory();
    expect(getHistory()).toEqual([]);
  });

  it('is idempotent on empty history', () => {
    clearHistory();
    expect(getHistory()).toEqual([]);
  });
});
