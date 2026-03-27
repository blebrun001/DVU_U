import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SourceEntry } from '../lib/types';
import { SourceManager } from './SourceManager';

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn()
}));

vi.mock('../lib/api', () => ({
  addSources: vi.fn(),
  removeSource: vi.fn()
}));

import { open } from '@tauri-apps/plugin-dialog';
import * as api from '../lib/api';

const mockedOpen = vi.mocked(open);
const mockedApi = vi.mocked(api);

const baseSource: SourceEntry = {
  id: 'source-1',
  path: '/tmp/file-a.txt',
  kind: 'file',
  recursive: true,
  addedAt: new Date().toISOString()
};

interface RenderOptions {
  sources?: SourceEntry[];
  totalBytes?: number;
  keepStructure?: boolean;
  disabled?: boolean;
}

function renderSourceManager(options: RenderOptions = {}) {
  const onSourcesChanged = vi.fn();
  const onKeepStructureChanged = vi.fn();

  render(
    <SourceManager
      sources={options.sources ?? [baseSource]}
      totalBytes={options.totalBytes ?? 128}
      onSourcesChanged={onSourcesChanged}
      keepStructure={options.keepStructure ?? false}
      onKeepStructureChanged={onKeepStructureChanged}
      disabled={options.disabled ?? false}
    />
  );

  return { onSourcesChanged, onKeepStructureChanged };
}

describe('SourceManager', () => {
  beforeEach(() => {
    mockedOpen.mockResolvedValue(['/tmp/file-b.txt']);
    mockedApi.addSources.mockResolvedValue([baseSource, { ...baseSource, id: 'source-2' }]);
    mockedApi.removeSource.mockResolvedValue({ ok: true });
  });

  it('shows lock feedback when disabled and user drops files', async () => {
    renderSourceManager({ disabled: true });

    const dropZone = screen.getByRole('button', { name: 'Drag & drop files or folders here' });
    fireEvent.drop(dropZone, { dataTransfer: { files: [] } });

    expect(screen.getByText('Sources are locked while transfer is active or paused.')).toBeInTheDocument();
    expect(mockedOpen).not.toHaveBeenCalled();
  });

  it('adds selected files and emits updated sources', async () => {
    const { onSourcesChanged } = renderSourceManager();

    fireEvent.click(screen.getByRole('button', { name: 'Add files' }));

    await waitFor(() => {
      expect(mockedOpen).toHaveBeenCalledOnce();
      expect(mockedApi.addSources).toHaveBeenCalledWith(['/tmp/file-b.txt'], true);
      expect(onSourcesChanged).toHaveBeenCalledOnce();
    });
  });

  it('shows API feedback when remove fails', async () => {
    mockedApi.removeSource.mockResolvedValueOnce({ ok: false, message: 'Cannot remove source' });
    renderSourceManager();

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }));

    await waitFor(() => {
      expect(mockedApi.removeSource).toHaveBeenCalledWith('source-1');
    });
    expect(screen.getByText('Cannot remove source')).toBeInTheDocument();
  });

  it('shows keep structure option when at least one folder is present', () => {
    renderSourceManager({
      sources: [{ ...baseSource, id: 'folder-1', kind: 'folder', path: '/tmp/folder' }]
    });

    expect(screen.getByLabelText('Keep structure (ZIP)')).toBeInTheDocument();
  });
});
