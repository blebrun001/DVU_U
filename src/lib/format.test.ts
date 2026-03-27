import { describe, expect, it } from 'vitest';

import { formatBytes, formatEta, formatRate } from './format';

describe('format helpers', () => {
  it('formats bytes in human readable units', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1024 * 1024)).toBe('1.0 MB');
  });

  it('formats throughput and eta', () => {
    expect(formatRate(4096)).toBe('4.0 KB/s');
    expect(formatEta(125)).toBe('2m 5s');
  });
});
