import { formatBytes } from '../lib/format';
import type { HistoryEntry } from '../lib/types';

interface HistoryPanelProps {
  history: HistoryEntry[];
  onRefresh: () => Promise<void>;
  onRestoreInterrupted: () => Promise<void>;
}

export function HistoryPanel({ history, onRefresh, onRestoreInterrupted }: HistoryPanelProps) {
  return (
    <section className="card">
      <header className="card-header">
        <h2>History</h2>
        <p>Completed runs and interrupted session recovery.</p>
      </header>

      <div className="actions-row">
        <button type="button" onClick={() => void onRefresh()}>
          Refresh history
        </button>
        <button type="button" onClick={() => void onRestoreInterrupted()}>
          Restore interrupted
        </button>
      </div>

      {history.length === 0 ? (
        <p className="empty-state">No completed sessions yet.</p>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Dataset</th>
                <th>State</th>
                <th>Uploaded</th>
                <th>Errors</th>
                <th>Total size</th>
                <th>Finished</th>
              </tr>
            </thead>
            <tbody>
              {history.map((entry) => (
                <tr key={entry.sessionId}>
                  <td>{entry.datasetPid}</td>
                  <td>{entry.state}</td>
                  <td>{entry.uploadedFiles}/{entry.totalFiles}</td>
                  <td>{entry.errorFiles}</td>
                  <td>{formatBytes(entry.totalBytes)}</td>
                  <td>{entry.finishedAt ?? '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
