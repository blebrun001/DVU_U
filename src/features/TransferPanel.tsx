import { formatBytes, formatEta, formatRate } from '../lib/format';
import type { AnalysisProgressEvent, FinalReport, SessionState, TransferSnapshot } from '../lib/types';
import { Stat } from '../components/Stat';

interface TransferPanelProps {
  sessionState: SessionState;
  snapshot: TransferSnapshot | null;
  analysisProgress: AnalysisProgressEvent | null;
  analysisLogs: string[];
  finalReport: FinalReport | null;
  canStart: boolean;
  onAction: (action: 'start' | 'pause' | 'resume' | 'cancel') => Promise<void>;
}

export function TransferPanel({
  sessionState,
  snapshot,
  analysisProgress,
  analysisLogs,
  finalReport,
  canStart,
  onAction
}: TransferPanelProps) {
  const isUploading = sessionState === 'uploading';
  const isResumable = sessionState === 'paused' || sessionState === 'interrupted';
  const canCancel =
    isUploading ||
    sessionState === 'paused' ||
    sessionState === 'scanning' ||
    sessionState === 'analyzing';

  return (
    <section className="card">
      <header className="card-header">
        <h2>Transfer</h2>
        <p>Start runs scan + analysis automatically, then uploads with retries and recovery.</p>
      </header>

      <div className="actions-row">
        <button type="button" className="primary" onClick={() => void onAction('start')} disabled={!canStart}>
          Start
        </button>
        <button type="button" onClick={() => void onAction('pause')} disabled={!isUploading}>
          Pause
        </button>
        <button type="button" onClick={() => void onAction('resume')} disabled={!isResumable}>
          Resume
        </button>
        <button type="button" className="danger" onClick={() => void onAction('cancel')} disabled={!canCancel}>
          Cancel
        </button>
      </div>

      <p className="mini-text">Session state: <strong>{sessionState}</strong></p>

      {(sessionState === 'scanning' || sessionState === 'analyzing') && analysisProgress && (
        <div className="analysis-box">
          <div className="stats-grid">
            <Stat
              label="Analysis progress"
              value={`Step ${analysisProgress.step}/${analysisProgress.totalSteps}`}
            />
            <Stat label="Current stage" value={analysisProgress.message} />
            <Stat
              label="State"
              value={sessionState === 'scanning' ? 'Scanning sources' : 'Analyzing batch'}
              muted
            />
          </div>
          <progress className="progress" value={analysisProgress.step} max={analysisProgress.totalSteps || 1} />
          {analysisLogs.length > 0 && (
            <div className="analysis-log">
              {analysisLogs.map((entry, index) => (
                <p key={`${index}-${entry}`} className="mini-text">{entry}</p>
              ))}
            </div>
          )}
        </div>
      )}

      {snapshot ? (
        <>
          <div className="stats-grid">
            <Stat label="Progress" value={`${snapshot.completedFiles}/${snapshot.totalFiles} files`} />
            <Stat
              label="Transferred"
              value={`${formatBytes(snapshot.uploadedBytes)} / ${formatBytes(snapshot.totalBytes)}`}
            />
            <Stat label="Throughput" value={formatRate(snapshot.throughputBytesPerSec)} />
            <Stat label="ETA" value={formatEta(snapshot.etaSeconds)} />
            <Stat label="Errors" value={snapshot.errorFiles} muted />
            <Stat label="Retrying" value={snapshot.retryingFiles} muted />
          </div>

          <progress
            className="progress"
            value={snapshot.totalBytes === 0 ? 0 : snapshot.uploadedBytes}
            max={snapshot.totalBytes || 1}
          />

          {snapshot.activeFile && (
            <div className="active-file">
              <strong>{snapshot.activeFile.fileName}</strong>
              <p>
                {snapshot.activeFile.state} - attempt {snapshot.activeFile.attempt} -{' '}
                {formatBytes(snapshot.activeFile.uploadedBytes)} / {formatBytes(snapshot.activeFile.totalBytes)}
              </p>
            </div>
          )}
          {snapshot.lastMessage && <p className="mini-text">{snapshot.lastMessage}</p>}
        </>
      ) : (
        <p className="empty-state">No transfer running.</p>
      )}

      {finalReport && (
        <div className="report-box">
          <h3>Final report</h3>
          <div className="stats-grid">
            <Stat label="Uploaded" value={finalReport.uploadedFiles} />
            <Stat label="Skipped" value={finalReport.skippedFiles} />
            <Stat label="Conflicts" value={finalReport.conflictFiles} />
            <Stat label="Errors" value={finalReport.errorFiles} />
            <Stat label="Total bytes" value={formatBytes(finalReport.totalBytes)} />
            <Stat label="Duration" value={finalReport.durationSeconds ? `${finalReport.durationSeconds}s` : 'n/a'} />
          </div>
        </div>
      )}
    </section>
  );
}
