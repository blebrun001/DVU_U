import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { TransferSnapshot } from '../lib/types';
import { TransferPanel } from './TransferPanel';

function makeSnapshot(state: TransferSnapshot['state']): TransferSnapshot {
  return {
    sessionId: 's-1',
    state,
    updatedAt: new Date().toISOString(),
    totalBytes: 100,
    uploadedBytes: 50,
    throughputBytesPerSec: 10,
    completedFiles: 1,
    totalFiles: 2,
    errorFiles: 0,
    retryingFiles: 0
  };
}

describe('TransferPanel controls', () => {
  it('enables start only when allowed', () => {
    render(
      <TransferPanel
        sessionState="ready"
        snapshot={null}
        analysisProgress={null}
        analysisLogs={[]}
        finalReport={null}
        canStart
        onAction={vi.fn(async () => undefined)}
        onExport={vi.fn(async () => undefined)}
      />
    );

    expect(screen.getByRole('button', { name: 'Start' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Pause' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Resume' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
  });

  it('enables pause and cancel during upload', () => {
    render(
      <TransferPanel
        sessionState="uploading"
        snapshot={makeSnapshot('uploading')}
        analysisProgress={null}
        analysisLogs={[]}
        finalReport={null}
        canStart={false}
        onAction={vi.fn(async () => undefined)}
        onExport={vi.fn(async () => undefined)}
      />
    );

    expect(screen.getByRole('button', { name: 'Start' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Pause' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  });

  it('enables resume for interrupted session', () => {
    render(
      <TransferPanel
        sessionState="interrupted"
        snapshot={makeSnapshot('interrupted')}
        analysisProgress={null}
        analysisLogs={[]}
        finalReport={null}
        canStart={false}
        onAction={vi.fn(async () => undefined)}
        onExport={vi.fn(async () => undefined)}
      />
    );

    expect(screen.getByRole('button', { name: 'Resume' })).toBeEnabled();
  });

  it('enables cancel during analysis', () => {
    render(
      <TransferPanel
        sessionState="analyzing"
        snapshot={null}
        analysisProgress={{ step: 3, totalSteps: 6, message: 'Preparing ZIP bundle' }}
        analysisLogs={['Preparing ZIP bundle']}
        finalReport={null}
        canStart={false}
        onAction={vi.fn(async () => undefined)}
        onExport={vi.fn(async () => undefined)}
      />
    );

    expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  });
});
