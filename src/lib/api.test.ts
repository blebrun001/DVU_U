import { describe, expect, it, vi } from 'vitest';
import { listRecentDatasets } from './api';

describe('listRecentDatasets', () => {
  it('returns empty list when server URL is blank', async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal('fetch', fetchSpy);

    const result = await listRecentDatasets({ serverUrl: '   ', apiToken: 'token' });

    expect(result).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('normalizes URL, adds token header, and maps Dataverse search payload', async () => {
    const fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        data: {
          items: [
            { global_id: 'doi:10.1/ABC', name: 'Dataset A' },
            { globalId: 'doi:10.1/DEF', title: 'Dataset B' },
            { name: 'Missing PID' }
          ]
        }
      })
    });
    vi.stubGlobal('fetch', fetchSpy);

    const result = await listRecentDatasets({
      serverUrl: 'https://demo.dataverse.org/dataverse/root/',
      apiToken: 'secret-token'
    });

    expect(result).toEqual([
      { persistentId: 'doi:10.1/ABC', title: 'Dataset A' },
      { persistentId: 'doi:10.1/DEF', title: 'Dataset B' }
    ]);
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    const [url, request] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(url).toContain('https://demo.dataverse.org/dataverse/root/api/search');
    expect(url).toContain('subtree=root');
    expect(request.method).toBe('GET');
    expect((request.headers as Record<string, string>)['X-Dataverse-Key']).toBe('secret-token');
  });

  it('falls back to origin URL when nested Dataverse route returns 404', async () => {
    const fetchSpy = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 404 })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          data: { items: [{ global_id: 'doi:10.1/GHI', title: 'Dataset C' }] }
        })
      });
    vi.stubGlobal('fetch', fetchSpy);

    const result = await listRecentDatasets({
      serverUrl: 'https://example.org/dataverse/demo',
      apiToken: ''
    });

    expect(result).toEqual([{ persistentId: 'doi:10.1/GHI', title: 'Dataset C' }]);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0][0]).toContain('https://example.org/dataverse/demo/api/search');
    expect(fetchSpy.mock.calls[1][0]).toContain('https://example.org/api/search');
  });

  it('throws when all HTTP attempts fail', async () => {
    const fetchSpy = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 500 })
      .mockResolvedValueOnce({ ok: false, status: 500 });
    vi.stubGlobal('fetch', fetchSpy);

    await expect(
      listRecentDatasets({
        serverUrl: 'https://example.org/dataverse/demo',
        apiToken: ''
      })
    ).rejects.toThrow('Dataverse recent dataset lookup failed (HTTP 500)');
  });
});
