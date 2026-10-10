import { describe, it, expect } from 'vitest';
import { tokenize, TokenType } from './tokenizer';

describe('tokenize', () => {
  describe('empty / whitespace input', () => {
    it('returns empty array for empty string', () => {
      expect(tokenize('')).toEqual([]);
    });

    it('returns empty array for whitespace-only string', () => {
      expect(tokenize('   ')).toEqual([]);
      expect(tokenize('\t\n')).toEqual([]);
    });
  });

  describe('domain token', () => {
    it('classifies the first token as Domain', () => {
      const [t] = tokenize('example.com');
      expect(t.type).toBe(TokenType.Domain);
      expect(t.value).toBe('example.com');
      expect(t.from).toBe(0);
      expect(t.to).toBe(11);
    });

    it('classifies the first token as Domain even if it looks like a record type', () => {
      const [t] = tokenize('MX');
      expect(t.type).toBe(TokenType.Domain);
    });

    it('classifies the first token as Domain even if it starts with @', () => {
      const [t] = tokenize('@example.com');
      expect(t.type).toBe(TokenType.Domain);
    });

    it('classifies the first token as Domain even if it starts with +', () => {
      const [t] = tokenize('+dnssec');
      expect(t.type).toBe(TokenType.Domain);
    });

    it('tracks correct byte offsets for a domain after leading whitespace', () => {
      const [t] = tokenize('  example.com');
      expect(t.from).toBe(2);
      expect(t.to).toBe(13);
    });
  });

  describe('record type tokens', () => {
    const types = ['A', 'AAAA', 'MX', 'TXT', 'NS', 'SOA', 'CNAME', 'CAA',
      'SRV', 'PTR', 'HTTPS', 'SVCB', 'SSHFP', 'TLSA', 'NAPTR',
      'HINFO', 'OPENPGPKEY', 'DNSKEY', 'DS', 'ALL'];

    for (const rtype of types) {
      it(`classifies ${rtype} as RecordType`, () => {
        const tokens = tokenize(`example.com ${rtype}`);
        expect(tokens[1].type).toBe(TokenType.RecordType);
        expect(tokens[1].value).toBe(rtype);
      });
    }

    it('classifies record types case-insensitively', () => {
      const tokens = tokenize('example.com mx aaaa txt');
      expect(tokens[1].type).toBe(TokenType.RecordType);
      expect(tokens[2].type).toBe(TokenType.RecordType);
      expect(tokens[3].type).toBe(TokenType.RecordType);
    });

    it('classifies mixed-case record types', () => {
      const tokens = tokenize('example.com Mx Txt Ns');
      expect(tokens[1].type).toBe(TokenType.RecordType);
      expect(tokens[2].type).toBe(TokenType.RecordType);
      expect(tokens[3].type).toBe(TokenType.RecordType);
    });

    it('classifies multiple record types in sequence', () => {
      const tokens = tokenize('example.com A AAAA MX');
      expect(tokens[1].type).toBe(TokenType.RecordType);
      expect(tokens[2].type).toBe(TokenType.RecordType);
      expect(tokens[3].type).toBe(TokenType.RecordType);
    });
  });

  describe('server tokens', () => {
    const servers = ['@cloudflare', '@google', '@quad9', '@mullvad', '@wikimedia', '@dns4eu', '@system'];

    for (const server of servers) {
      it(`classifies ${server} as Server`, () => {
        const tokens = tokenize(`example.com ${server}`);
        expect(tokens[1].type).toBe(TokenType.Server);
        expect(tokens[1].value).toBe(server);
      });
    }

    it('classifies a bare @ prefix token as Server', () => {
      const tokens = tokenize('example.com @unknown-resolver');
      expect(tokens[1].type).toBe(TokenType.Server);
    });

    it('classifies multiple server tokens', () => {
      const tokens = tokenize('example.com @cloudflare @google');
      expect(tokens[1].type).toBe(TokenType.Server);
      expect(tokens[2].type).toBe(TokenType.Server);
    });
  });

  describe('flag tokens', () => {
    const flags = ['+dnssec', '+tcp', '+udp', '+tls', '+https', '+check', '+trace'];

    for (const flag of flags) {
      it(`classifies ${flag} as Flag`, () => {
        const tokens = tokenize(`example.com ${flag}`);
        expect(tokens[1].type).toBe(TokenType.Flag);
        expect(tokens[1].value).toBe(flag);
      });
    }

    it('classifies a bare + prefix token as Flag', () => {
      const tokens = tokenize('example.com +unknown-flag');
      expect(tokens[1].type).toBe(TokenType.Flag);
    });

    it('classifies multiple flag tokens', () => {
      const tokens = tokenize('example.com +dnssec +tcp');
      expect(tokens[1].type).toBe(TokenType.Flag);
      expect(tokens[2].type).toBe(TokenType.Flag);
    });
  });

  describe('unknown tokens', () => {
    it('classifies unrecognized words as Unknown', () => {
      const tokens = tokenize('example.com notarecordtype');
      expect(tokens[1].type).toBe(TokenType.Unknown);
      expect(tokens[1].value).toBe('notarecordtype');
    });

    it('classifies numbers as Unknown', () => {
      const tokens = tokenize('example.com 12345');
      expect(tokens[1].type).toBe(TokenType.Unknown);
    });
  });

  describe('byte offsets', () => {
    it('computes correct from/to for each token', () => {
      const tokens = tokenize('example.com A @cloudflare +dnssec');
      expect(tokens[0]).toMatchObject({ value: 'example.com', from: 0, to: 11 });
      expect(tokens[1]).toMatchObject({ value: 'A', from: 12, to: 13 });
      expect(tokens[2]).toMatchObject({ value: '@cloudflare', from: 14, to: 25 });
      expect(tokens[3]).toMatchObject({ value: '+dnssec', from: 26, to: 33 });
    });

    it('handles multiple spaces between tokens correctly', () => {
      const tokens = tokenize('example.com   MX');
      expect(tokens[0].from).toBe(0);
      expect(tokens[1].from).toBe(14);
      expect(tokens[1].to).toBe(16);
    });
  });

  describe('full query combinations', () => {
    it('parses a typical multi-type multi-server query', () => {
      const tokens = tokenize('example.com A MX @cloudflare @google +dnssec');
      expect(tokens).toHaveLength(6);
      expect(tokens[0].type).toBe(TokenType.Domain);
      expect(tokens[1].type).toBe(TokenType.RecordType);
      expect(tokens[2].type).toBe(TokenType.RecordType);
      expect(tokens[3].type).toBe(TokenType.Server);
      expect(tokens[4].type).toBe(TokenType.Server);
      expect(tokens[5].type).toBe(TokenType.Flag);
    });

    it('parses a domain-only query', () => {
      const tokens = tokenize('example.com');
      expect(tokens).toHaveLength(1);
      expect(tokens[0].type).toBe(TokenType.Domain);
    });

    it('handles subdomain with port-style notation as a single unknown token', () => {
      const tokens = tokenize('example.com notvalid');
      expect(tokens[1].type).toBe(TokenType.Unknown);
    });
  });
});
