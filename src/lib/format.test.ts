import { describe, expect, it } from 'vitest';
import { formatBytes, formatDuration, formatPercent } from './format';

describe('format utilities', () => {
  it('formats byte sizes cleanly', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1_500_000)).toBe('1.50 MB');
    expect(formatBytes(2_900_000_000)).toBe('2.90 GB');
  });

  it('formats durations cleanly', () => {
    expect(formatDuration(450)).toBe('450 ms');
    expect(formatDuration(1420)).toBe('1.42 s');
    expect(formatDuration(125_000)).toBe('2m 05s');
  });

  it('formats percentages cleanly', () => {
    expect(formatPercent(0)).toBe('0%');
    expect(formatPercent(0.02)).toBe('<0.1%');
    expect(formatPercent(48.35)).toBe('48.4%');
  });
});
