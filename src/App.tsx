import { useEffect } from 'react';
import { exportReport } from './lib/api';
import { DestinationForm } from './features/DestinationForm';
import { HistoryPanel } from './features/HistoryPanel';
import { SourceManager } from './features/SourceManager';
import { TransferPanel } from './features/TransferPanel';
import { teardownStoreListener, useAppStore } from './store/appStore';

export function App() {
  const store = useAppStore();
  const transferLocked =
    store.sessionState === 'uploading' ||
    store.sessionState === 'paused' ||
    store.sessionState === 'cancelling';
  const canStartTransfer =
    store.sources.length > 0 &&
    !transferLocked &&
    store.sessionState !== 'scanning' &&
    store.sessionState !== 'analyzing';

  useEffect(() => {
    void store.bootstrap();
    const interval = setInterval(() => {
      void store.refreshSnapshot();
    }, 1500);
    return () => {
      clearInterval(interval);
      teardownStoreListener();
    };
  }, []);

  return (
    <main className="app-shell">
      <header className="hero">
        <h1>Dataverse Heavy Uploader</h1>
        <p>Reliable large-file transfer with analysis, retry, and recovery.</p>
      </header>

      {store.errorMessage && <div className="global-error">{store.errorMessage}</div>}
      {store.sessionState === 'interrupted' && (
        <div className="global-warning">
          Previous transfer was interrupted. Review progress and click Resume when ready.
        </div>
      )}

      <div className="layout-grid">
        <DestinationForm
          initialDestination={store.destination}
          disabled={transferLocked}
        />
        <SourceManager
          sources={store.sources}
          totalBytes={store.scanSummary?.totalBytes ?? 0}
          onSourcesChanged={store.setSources}
          keepStructure={store.keepStructure}
          onKeepStructureChanged={store.setKeepStructure}
          disabled={transferLocked}
        />
      </div>

      <div className="layout-stack">
        <TransferPanel
          sessionState={store.sessionState}
          snapshot={store.snapshot}
          analysisProgress={store.analysisProgress}
          analysisLogs={store.analysisLogs}
          finalReport={store.finalReport}
          canStart={canStartTransfer}
          onAction={store.transferAction}
          onExport={async (format) => {
            await exportReport(format);
          }}
        />

        <HistoryPanel
          history={store.history}
          onRefresh={store.refreshHistory}
          onRestoreInterrupted={store.restoreInterrupted}
        />
      </div>
    </main>
  );
}
