import { useEffect, useState } from 'react';
import { DestinationForm } from './features/DestinationForm';
import { HistoryPanel } from './features/HistoryPanel';
import { SourceManager } from './features/SourceManager';
import { TransferPanel } from './features/TransferPanel';
import { shouldPollSnapshot, teardownStoreListener, useAppStore } from './store/appStore';

export function App() {
  const store = useAppStore();
  const [resetSignal, setResetSignal] = useState(0);
  const transferLocked =
    store.sessionState === 'uploading' ||
    store.sessionState === 'paused' ||
    store.sessionState === 'cancelling';
  const canStartTransfer =
    store.sources.length > 0 &&
    !transferLocked &&
    store.sessionState !== 'scanning' &&
    store.sessionState !== 'analyzing';
  const handleResetInterface = async () => {
    await store.resetInterface();
    setResetSignal((value) => value + 1);
  };

  useEffect(() => {
    void store.bootstrap();
    return () => {
      teardownStoreListener();
    };
  }, []);

  useEffect(() => {
    if (!shouldPollSnapshot(store.sessionState)) {
      return;
    }
    const interval = setInterval(() => {
      void store.refreshSnapshot();
    }, 1500);
    return () => {
      clearInterval(interval);
    };
  }, [store.sessionState, store.refreshSnapshot]);

  return (
    <main className="app-shell">
      <header className="hero">
        <img
          src="/favicon.png"
          alt="DVU_U logo"
          className="hero-logo"
        />
        <div className="hero-text">
          <h1>Dataverse Uploader Universal (DVU_U)</h1>
          <p>Reliable large-file transfer with analysis, retry, and recovery.</p>
        </div>
        <div className="hero-actions">
          <button type="button" className="ghost" onClick={handleResetInterface} disabled={store.isBusy}>
            Réinitialiser l'interface
          </button>
        </div>
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
          resetSignal={resetSignal}
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
        />

        <HistoryPanel
          history={store.history}
          onRefresh={store.refreshHistory}
          onRestoreInterrupted={store.restoreInterrupted}
        />
      </div>

      <section id="about" className="about" aria-label="About">
        <p>
          made by Brice Lebrun under the GPL-3.0 license - Institut Català de
          Paleoecologia Humana i Evolució Social - Zona Educacional 4 Campus
          Sescelades URV (Edifici W3) 43007 - TARRAGONA
        </p>
      </section>
    </main>
  );
}
