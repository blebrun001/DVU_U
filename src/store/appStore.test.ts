import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  AnalysisProgressEvent,
  BootstrapState,
  ScanSummary,
  SessionState,
  SourceEntry,
  TransferPlan,
  TransferSnapshot
} from '../lib/types';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => async () => undefined)
}));

vi.mock('../lib/api', () => ({
  analyzeBatch: vi.fn(),
  cancelTransfer: vi.fn(),
  getFinalReport: vi.fn(),
  getTransferSnapshot: vi.fn(),
  listHistory: vi.fn(),
  loadBootstrapState: vi.fn(),
  pauseTransfer: vi.fn(),
  restoreLastInterrupted: vi.fn(),
  resumeTransfer: vi.fn(),
  scanSources: vi.fn(),
  startTransfer: vi.fn()
}));

import * as api from '../lib/api';
import { canKeepStructureForSources, useAppStore } from './appStore';

const mockedApi = vi.mocked(api);

const defaultBootstrap: BootstrapState = {
  sessionId: 'session-1',
  sessionState: 'draft',
  sources: []
};

const sourceFile: SourceEntry = {
  id: 'src-1',
  path: '/tmp/file-a.txt',
  kind: 'file',
  recursive: true,
  addedAt: new Date().toISOString()
};

const folderSource: SourceEntry = {
  id: 'src-folder',
  path: '/tmp/folder',
  kind: 'folder',
  recursive: true,
  addedAt: new Date().toISOString()
};

function makeScanSummary(totalFiles = 2): ScanSummary {
  return {
    totalFiles,
    totalBytes: 1024,
    unreadableCount: 0,
    ignoredSymlinkCount: 0,
    duplicatePathCount: 0
  };
}

function makePlan(toUploadFiles = 2, blockingErrors: string[] = []): TransferPlan {
  return {
    sessionId: 'session-1',
    createdAt: new Date().toISOString(),
    summary: {
      totalFiles: 2,
      totalBytes: 1024,
      toUploadFiles,
      toUploadBytes: 1024,
      skippedExistingFiles: 0,
      conflictFiles: 0,
      ignoredFiles: 0,
      errorFiles: 0,
      blockingErrors
    },
    items: []
  };
}

function makeSnapshot(state: SessionState): TransferSnapshot {
  return {
    sessionId: 'session-1',
    state,
    updatedAt: new Date().toISOString(),
    totalBytes: 1024,
    uploadedBytes: 128,
    throughputBytesPerSec: 32,
    completedFiles: 1,
    totalFiles: 2,
    errorFiles: 0,
    retryingFiles: 0
  };
}

function resetStoreState() {
  useAppStore.setState({
    initialized: false,
    isBusy: false,
    sessionId: '',
    sessionState: 'draft',
    destination: null,
    sources: [],
    keepStructure: false,
    scanSummary: null,
    transferPlan: null,
    snapshot: null,
    analysisProgress: null,
    analysisLogs: [],
    finalReport: null,
    history: [],
    errorMessage: null,
    listenerReady: false
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  resetStoreState();
  mockedApi.loadBootstrapState.mockResolvedValue(defaultBootstrap);
  mockedApi.listHistory.mockResolvedValue([]);
  mockedApi.scanSources.mockResolvedValue(makeScanSummary());
  mockedApi.analyzeBatch.mockResolvedValue(makePlan());
  mockedApi.startTransfer.mockResolvedValue({ ok: true });
  mockedApi.pauseTransfer.mockResolvedValue({ ok: true });
  mockedApi.resumeTransfer.mockResolvedValue({ ok: true });
  mockedApi.cancelTransfer.mockResolvedValue({ ok: true });
  mockedApi.getTransferSnapshot.mockResolvedValue(makeSnapshot('uploading'));
  mockedApi.getFinalReport.mockResolvedValue(null);
});

describe('canKeepStructureForSources', () => {
  it('returns false for a single file', () => {
    expect(canKeepStructureForSources([sourceFile])).toBe(false);
  });

  it('returns true for multiple sources or a folder', () => {
    expect(canKeepStructureForSources([sourceFile, { ...sourceFile, id: 'src-2' }])).toBe(true);
    expect(canKeepStructureForSources([folderSource])).toBe(true);
  });
});

describe('useAppStore setSources', () => {
  it('resets derived transfer states and enables keepStructure when it becomes available', () => {
    useAppStore.setState({
      keepStructure: false,
      scanSummary: makeScanSummary(4),
      transferPlan: makePlan(3),
      analysisProgress: { step: 2, totalSteps: 6, message: 'Analyzing' } as AnalysisProgressEvent,
      analysisLogs: ['log'],
      finalReport: {
        sessionId: 'session-1',
        totalFiles: 1,
        uploadedFiles: 1,
        skippedFiles: 0,
        conflictFiles: 0,
        errorFiles: 0,
        cancelledFiles: 0,
        totalBytes: 10,
        uploadedBytes: 10,
        entries: []
      }
    });

    useAppStore.getState().setSources([folderSource]);

    const state = useAppStore.getState();
    expect(state.keepStructure).toBe(true);
    expect(state.scanSummary).toBeNull();
    expect(state.transferPlan).toBeNull();
    expect(state.analysisProgress).toBeNull();
    expect(state.analysisLogs).toEqual([]);
    expect(state.finalReport).toBeNull();
  });

  it('forces keepStructure off when source list no longer supports it', () => {
    useAppStore.setState({
      sources: [folderSource],
      keepStructure: true
    });

    useAppStore.getState().setSources([sourceFile]);

    expect(useAppStore.getState().keepStructure).toBe(false);
  });
});

describe('useAppStore transferAction', () => {
  it('runs start flow and transitions to uploading', async () => {
    useAppStore.setState({
      sources: [sourceFile],
      keepStructure: true
    });

    await useAppStore.getState().transferAction('start');

    const state = useAppStore.getState();
    expect(mockedApi.scanSources).toHaveBeenCalledOnce();
    expect(mockedApi.analyzeBatch).toHaveBeenCalledWith({ keepStructure: true });
    expect(mockedApi.startTransfer).toHaveBeenCalledOnce();
    expect(state.sessionState).toBe('uploading');
    expect(state.scanSummary?.totalFiles).toBe(2);
    expect(state.transferPlan?.summary.toUploadFiles).toBe(2);
    expect(state.errorMessage).toBeNull();
    expect(state.isBusy).toBe(false);
  });

  it('records cancellation cleanly when user cancels during start sequence', async () => {
    mockedApi.analyzeBatch.mockRejectedValueOnce(new Error('Operation cancelled by user'));
    useAppStore.setState({ sources: [sourceFile] });

    await useAppStore.getState().transferAction('start');

    const state = useAppStore.getState();
    expect(state.sessionState).toBe('draft');
    expect(state.errorMessage).toBeNull();
    expect(state.analysisProgress?.message).toBe('Analysis cancelled by user');
  });

  it('captures action errors with explicit message', async () => {
    mockedApi.pauseTransfer.mockRejectedValueOnce(new Error('network down'));

    await useAppStore.getState().transferAction('pause');

    const state = useAppStore.getState();
    expect(state.errorMessage).toContain('Transfer action failed (pause): Error: network down');
    expect(state.isBusy).toBe(false);
  });
});
