import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}));

import { invoke } from '@tauri-apps/api/core';
import { listRecentDatasets } from './api';

const mockedInvoke = vi.mocked(invoke);

describe('listRecentDatasets', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns empty list when server URL is blank', async () => {
    const result = await listRecentDatasets({ serverUrl: '   ', apiToken: 'token' });
    expect(result).toEqual([]);
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it('uses backend IPC command and preserves response shape', async () => {
    mockedInvoke.mockResolvedValueOnce([
      { persistentId: 'doi:10.1/ABC', title: 'Dataset A' },
      { persistentId: 'doi:10.1/DEF', title: 'Dataset B' }
    ]);

    const result = await listRecentDatasets({
      serverUrl: 'https://demo.dataverse.org/dataverse/root/',
      apiToken: 'secret-token'
    });

    expect(mockedInvoke).toHaveBeenCalledWith('list_recent_datasets', {
      input: {
        serverUrl: 'https://demo.dataverse.org/dataverse/root/',
        apiToken: 'secret-token'
      }
    });
    expect(result).toEqual([
      { persistentId: 'doi:10.1/ABC', title: 'Dataset A' },
      { persistentId: 'doi:10.1/DEF', title: 'Dataset B' }
    ]);
  });

  it('propagates backend errors', async () => {
    mockedInvoke.mockRejectedValueOnce(new Error('Dataverse recent dataset lookup failed'));

    await expect(
      listRecentDatasets({
        serverUrl: 'https://example.org',
        apiToken: ''
      })
    ).rejects.toThrow('Dataverse recent dataset lookup failed');
  });
});
