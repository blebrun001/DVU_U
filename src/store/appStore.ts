import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';
import {
  analyzeBatch,
  cancelTransfer,
  getFinalReport,
  getTransferSnapshot,
  listHistory,
  loadBootstrapState,
  pauseTransfer,
  restoreLastInterrupted,
  resumeTransfer,
  scanSources,
  startTransfer
} from '../lib/api';
import type {
  AnalysisProgressEvent,
  BootstrapState,
  DestinationBootstrap,
  FinalReport,
  HistoryEntry,
  ScanSummary,
  SessionState,
  SourceEntry,
  TransferPlan,
  TransferSnapshot
} from '../lib/types';

interface AppStore {
  initialized: boolean;
  isBusy: boolean;
  sessionId: string;
  sessionState: SessionState;
  destination: DestinationBootstrap | null;
  sources: SourceEntry[];
  keepStructure: boolean;
  scanSummary: ScanSummary | null;
  transferPlan: TransferPlan | null;
  snapshot: TransferSnapshot | null;
  analysisProgress: AnalysisProgressEvent | null;
  analysisLogs: string[];
  finalReport: FinalReport | null;
  history: HistoryEntry[];
  errorMessage: string | null;
  listenerReady: boolean;
  bootstrap: () => Promise<void>;
  setSources: (sources: SourceEntry[]) => void;
  setKeepStructure: (value: boolean) => void;
  refreshScanSummary: () => Promise<void>;
  refreshSnapshot: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  restoreInterrupted: () => Promise<void>;
  transferAction: (action: 'start' | 'pause' | 'resume' | 'cancel') => Promise<void>;
}

let detachListeners: UnlistenFn[] = [];

export function canKeepStructureForSources(sources: SourceEntry[]): boolean {
  return sources.length > 1 || sources.some((source) => source.kind === 'folder');
}

function fromBootstrap(bootstrap: BootstrapState) {
  return {
    sessionId: bootstrap.sessionId,
    sessionState: bootstrap.sessionState,
    destination: bootstrap.destination ?? null,
    sources: bootstrap.sources,
    keepStructure: canKeepStructureForSources(bootstrap.sources),
    scanSummary: bootstrap.scanSummary ?? null,
    transferPlan: bootstrap.transferPlan ?? null,
    snapshot: bootstrap.lastSnapshot ?? null,
    finalReport: bootstrap.finalReport ?? null
  };
}

export const useAppStore = create<AppStore>((set, get) => ({
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
  listenerReady: false,
  bootstrap: async () => {
    set({ isBusy: true, errorMessage: null });
    try {
      const [bootstrap, history] = await Promise.all([loadBootstrapState(), listHistory()]);
      set({ ...fromBootstrap(bootstrap), history, initialized: true });
      if (!get().listenerReady) {
        const snapshotListener = await listen<TransferSnapshot>('transfer:snapshot', (event) => {
          const snapshot = event.payload;
          set({
            snapshot,
            sessionState: snapshot.state
          });
          if (
            snapshot.state === 'completed' ||
            snapshot.state === 'completed_with_errors' ||
            snapshot.state === 'failed'
          ) {
            void get().refreshSnapshot();
          }
        });
        const analysisListener = await listen<AnalysisProgressEvent>('analysis:progress', (event) => {
          const progress = event.payload;
          set((state) => {
            const nextLogs = [...state.analysisLogs, progress.message];
            return {
              analysisProgress: progress,
              analysisLogs: nextLogs.slice(-8)
            };
          });
        });
        detachListeners = [snapshotListener, analysisListener];
        set({ listenerReady: true });
      }
    } catch (err) {
      set({ errorMessage: `Failed to initialize app: ${String(err)}` });
    } finally {
      set({ isBusy: false });
    }
  },
  setSources: (sources) => {
    const previousSources = get().sources;
    const previousCanKeepStructure = canKeepStructureForSources(previousSources);
    const nextCanKeepStructure = canKeepStructureForSources(sources);
    let keepStructure = get().keepStructure;
    if (!nextCanKeepStructure) {
      keepStructure = false;
    } else if (!previousCanKeepStructure && nextCanKeepStructure) {
      keepStructure = true;
    }
    set({
      sources,
      keepStructure,
      scanSummary: null,
      transferPlan: null,
      analysisProgress: null,
      analysisLogs: [],
      finalReport: null,
      errorMessage: null
    });
  },
  setKeepStructure: (value) => set({ keepStructure: value }),
  refreshScanSummary: async () => {
    set({
      isBusy: true,
      errorMessage: null,
      sessionState: 'scanning',
      transferPlan: null,
      analysisProgress: { step: 1, totalSteps: 2, message: 'Scanning sources' },
      analysisLogs: ['Scanning sources'],
      finalReport: null
    });
    try {
      const summary = await scanSources();
      set({
        scanSummary: summary,
        sessionState: 'draft',
        analysisProgress: { step: 2, totalSteps: 2, message: `Scan complete: ${summary.totalFiles} files found` },
        analysisLogs: [`Scan complete: ${summary.totalFiles} files found`]
      });
    } catch (err) {
      set({ errorMessage: `Scan failed: ${String(err)}`, sessionState: 'draft' });
    } finally {
      set({ isBusy: false });
    }
  },
  refreshSnapshot: async () => {
    try {
      const [snapshot, report] = await Promise.all([getTransferSnapshot(), getFinalReport()]);
      set({ snapshot, finalReport: report });
      if (snapshot) {
        set({ sessionState: snapshot.state });
        if (
          snapshot.state === 'completed' ||
          snapshot.state === 'completed_with_errors' ||
          snapshot.state === 'failed'
        ) {
          await get().refreshHistory();
        }
      }
    } catch (err) {
      set({ errorMessage: `Cannot refresh transfer snapshot: ${String(err)}` });
    }
  },
  refreshHistory: async () => {
    try {
      const history = await listHistory();
      set({ history });
    } catch (err) {
      set({ errorMessage: `Cannot load history: ${String(err)}` });
    }
  },
  restoreInterrupted: async () => {
    set({ isBusy: true, errorMessage: null });
    try {
      await restoreLastInterrupted();
      const bootstrap = await loadBootstrapState();
      set({ ...fromBootstrap(bootstrap) });
    } catch (err) {
      set({ errorMessage: `Restore failed: ${String(err)}` });
    } finally {
      set({ isBusy: false });
    }
  },
  transferAction: async (action) => {
    set({ isBusy: true, errorMessage: null });
    try {
      if (action === 'start') {
        if (get().sources.length === 0) {
          throw new Error('Add at least one source before starting transfer.');
        }

        set({
          sessionState: 'scanning',
          analysisProgress: { step: 1, totalSteps: 6, message: 'Scanning sources' },
          analysisLogs: ['Scanning sources']
        });
        const currentScan = get().scanSummary;
        if (!currentScan || currentScan.totalFiles === 0) {
          const summary = await scanSources();
          set((state) => ({
            scanSummary: summary,
            analysisProgress: { step: 2, totalSteps: 6, message: `Scan complete: ${summary.totalFiles} files found` },
            analysisLogs: [...state.analysisLogs, `Scan complete: ${summary.totalFiles} files found`].slice(-8)
          }));
        } else {
          set((state) => ({
            analysisProgress: {
              step: 2,
              totalSteps: 6,
              message: `Using previous scan: ${currentScan.totalFiles} files`
            },
            analysisLogs: [...state.analysisLogs, `Using previous scan: ${currentScan.totalFiles} files`].slice(
              -8
            )
          }));
        }

        set({ sessionState: 'analyzing' });
        const transferPlan = await analyzeBatch({
          keepStructure: get().keepStructure
        });
        if (
          (transferPlan.summary.toUploadFiles ?? 0) === 0 ||
          (transferPlan.summary.blockingErrors.length ?? 0) > 0
        ) {
          set({
            transferPlan,
            sessionState: 'draft'
          });
          throw new Error(
            transferPlan.summary.blockingErrors[0] ??
              'No files are eligible for upload after automatic analysis.'
          );
        }

        set((state) => ({
          transferPlan,
          sessionState: 'ready',
          analysisProgress: { step: 6, totalSteps: 6, message: 'Analysis completed, starting upload' },
          analysisLogs: [...state.analysisLogs, 'Analysis completed, starting upload'].slice(-8),
          finalReport: null
        }));
        await startTransfer();
      } else if (action === 'pause') {
        await pauseTransfer();
      } else if (action === 'resume') {
        await resumeTransfer();
      } else {
        await cancelTransfer();
      }
      await get().refreshSnapshot();
      if (action === 'cancel') {
        await get().refreshHistory();
      }
      if (action === 'start' || action === 'resume') {
        set({ sessionState: 'uploading' });
      }
    } catch (err) {
      const message = String(err);
      const cancelledByUser =
        message.toLowerCase().includes('operation cancelled by user') ||
        message.toLowerCase().includes('cancellation requested');
      if (action === 'start' && cancelledByUser) {
        set({
          sessionState: 'draft',
          errorMessage: null,
          analysisProgress: { step: 6, totalSteps: 6, message: 'Analysis cancelled by user' },
          analysisLogs: [...get().analysisLogs, 'Analysis cancelled by user'].slice(-8)
        });
      } else {
        set({ errorMessage: `Transfer action failed (${action}): ${message}` });
      }
    } finally {
      set({ isBusy: false });
    }
  }
}));

export function teardownStoreListener() {
  for (const detach of detachListeners) {
    void detach();
  }
  detachListeners = [];
}
