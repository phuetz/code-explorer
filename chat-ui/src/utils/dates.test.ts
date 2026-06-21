import { describe, expect, it } from 'vitest';
import { parseIndexedAt } from './dates';

describe('parseIndexedAt', () => {
  it('parses ISO 8601 strings', () => {
    const d = parseIndexedAt('2026-04-17T05:58:45Z');
    expect(d).not.toBeNull();
    expect(d?.getUTCFullYear()).toBe(2026);
    expect(d?.getUTCMonth()).toBe(3); // April = 3
    expect(d?.getUTCDate()).toBe(17);
  });

  it('parses Unix epoch seconds with Z trailing', () => {
    const d = parseIndexedAt('1774507049Z');
    expect(d).not.toBeNull();
    expect(d?.getUTCFullYear()).toBe(2026);
  });

  it('parses Unix epoch seconds without Z', () => {
    const d = parseIndexedAt('1774507049');
    expect(d).not.toBeNull();
    expect(d?.getUTCFullYear()).toBe(2026);
  });

  it('returns null for empty/null/undefined', () => {
    expect(parseIndexedAt(undefined)).toBeNull();
    expect(parseIndexedAt(null)).toBeNull();
    expect(parseIndexedAt('')).toBeNull();
  });

  it('returns null for non-parseable garbage', () => {
    expect(parseIndexedAt('not-a-date')).toBeNull();
    expect(parseIndexedAt('123')).toBeNull(); // too small for unix epoch
  });

  it('rejects sub-2001 epoch values', () => {
    // 1_000_000_000 = 2001-09-09 ; tout en dessous est invalide.
    expect(parseIndexedAt('999999999')).toBeNull();
  });
});
