import { describe, expect, it } from 'vitest';
import { parseBatchEvent } from './parseBatchEvent';

describe('parseBatchEvent', () => {
  it('flattens the nested lookups structure', () => {
    const raw = {
      request_id: 'abc-123',
      record_type: 'A',
      lookups: {
        lookups: [
          {
            query: { name: 'example.com.', record_type: 'A' },
            name_server: 'udp:1.1.1.1:53',
            result: {
              Response: {
                records: [{ name: 'example.com.', type: 'A', ttl: 300, data: { A: '93.184.216.34' } }],
                response_time: { secs: 0, nanos: 12000000 },
              },
            },
          },
        ],
      },
      completed: 1,
      total: 4,
    };

    const batch = parseBatchEvent(raw);
    expect(batch.request_id).toBe('abc-123');
    expect(batch.record_type).toBe('A');
    expect(batch.lookups).toHaveLength(1);
    expect(batch.lookups[0].name_server).toBe('udp:1.1.1.1:53');
    expect(batch.completed).toBe(1);
    expect(batch.total).toBe(4);
  });

  it('handles missing lookups gracefully', () => {
    const raw = {
      record_type: 'AAAA',
      lookups: { lookups: [] },
      completed: 1,
      total: 1,
    };

    const batch = parseBatchEvent(raw);
    expect(batch.lookups).toEqual([]);
    expect(batch.request_id).toBeUndefined();
  });

  it('handles null lookups field gracefully', () => {
    const raw = {
      record_type: 'MX',
      lookups: null as unknown as { lookups: [] },
      completed: 1,
      total: 2,
    };

    const batch = parseBatchEvent(raw);
    expect(batch.lookups).toEqual([]);
  });

  it('preserves error result variants', () => {
    const raw = {
      request_id: 'def-456',
      record_type: 'TXT',
      lookups: {
        lookups: [
          {
            query: { name: 'example.com.', record_type: 'TXT' },
            name_server: 'udp:8.8.8.8:53',
            result: { NxDomain: { response_time: { secs: 0, nanos: 5000000 } } },
          },
        ],
      },
      completed: 2,
      total: 4,
    };

    const batch = parseBatchEvent(raw);
    expect(batch.lookups).toHaveLength(1);
    expect(batch.lookups[0].result).toHaveProperty('NxDomain');
  });

  it('handles multiple lookups from different servers', () => {
    const raw = {
      record_type: 'NS',
      lookups: {
        lookups: [
          {
            query: { name: 'example.com.', record_type: 'NS' },
            name_server: 'udp:1.1.1.1:53',
            result: {
              Response: {
                records: [{ name: 'example.com.', type: 'NS', ttl: 3600, data: { NS: 'ns1.example.com.' } }],
                response_time: { secs: 0, nanos: 10000000 },
              },
            },
          },
          {
            query: { name: 'example.com.', record_type: 'NS' },
            name_server: 'udp:8.8.8.8:53',
            result: {
              Response: {
                records: [{ name: 'example.com.', type: 'NS', ttl: 3600, data: { NS: 'ns2.example.com.' } }],
                response_time: { secs: 0, nanos: 15000000 },
              },
            },
          },
        ],
      },
      completed: 3,
      total: 4,
    };

    const batch = parseBatchEvent(raw);
    expect(batch.lookups).toHaveLength(2);
    expect(batch.lookups[0].name_server).toBe('udp:1.1.1.1:53');
    expect(batch.lookups[1].name_server).toBe('udp:8.8.8.8:53');
  });
});
