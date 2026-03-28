import { useState, type DragEvent } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { addSources, removeSource } from '../lib/api';
import { formatBytes } from '../lib/format';
import type { SourceEntry } from '../lib/types';

interface SourceManagerProps {
  sources: SourceEntry[];
  totalBytes: number;
  onSourcesChanged: (sources: SourceEntry[]) => void;
  keepStructure: boolean;
  onKeepStructureChanged: (value: boolean) => void;
  disabled?: boolean;
}

export function SourceManager({
  sources,
  totalBytes,
  onSourcesChanged,
  keepStructure,
  onKeepStructureChanged,
  disabled = false
}: SourceManagerProps) {
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const canKeepStructure = sources.length > 1 || sources.some((source) => source.kind === 'folder');

  const pickFiles = async () => {
    if (disabled) {
      setFeedback('Sources are locked while transfer is active or paused.');
      return;
    }
    setBusy(true);
    setFeedback(null);
    try {
      const selected = await open({
        multiple: true,
        directory: false
      });
      const paths = normalizeSelection(selected);
      if (paths.length === 0) {
        return;
      }
      const updated = await addSources(paths, true);
      onSourcesChanged(updated);
    } catch (err) {
      setFeedback(`Cannot add files: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const pickFolder = async () => {
    if (disabled) {
      setFeedback('Sources are locked while transfer is active or paused.');
      return;
    }
    setBusy(true);
    setFeedback(null);
    try {
      const selected = await open({
        multiple: true,
        directory: true
      });
      const paths = normalizeSelection(selected);
      if (paths.length === 0) {
        return;
      }
      const updated = await addSources(paths, true);
      onSourcesChanged(updated);
    } catch (err) {
      setFeedback(`Cannot add folders: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (sourceId: string) => {
    if (disabled) {
      setFeedback('Sources are locked while transfer is active or paused.');
      return;
    }
    setBusy(true);
    setFeedback(null);
    try {
      const result = await removeSource(sourceId);
      if (!result.ok) {
        setFeedback(result.message ?? 'Cannot remove source');
        return;
      }
      onSourcesChanged(sources.filter((item) => item.id !== sourceId));
    } catch (err) {
      setFeedback(`Cannot remove source: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const onDrop = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    if (disabled) {
      setFeedback('Sources are locked while transfer is active or paused.');
      return;
    }
    const paths = Array.from(event.dataTransfer.files)
      .map((file) => {
        const withPath = file as File & { path?: string };
        return withPath.path;
      })
      .filter((item): item is string => Boolean(item));

    if (paths.length === 0) {
      setFeedback('Dropped files are not readable by path on this platform. Use Add files/folder.');
      return;
    }

    setBusy(true);
    try {
      const updated = await addSources(paths, true);
      onSourcesChanged(updated);
    } catch (err) {
      setFeedback(`Cannot import dropped files: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="card">
      <header className="card-header">
        <h2>Sources</h2>
        <p>Add files and folders to prepare your upload batch.</p>
      </header>

      <div className="actions-row">
        <button type="button" onClick={pickFiles} disabled={busy || disabled}>
          Add files
        </button>
        <button type="button" onClick={pickFolder} disabled={busy || disabled}>
          Add folder
        </button>
        {canKeepStructure && (
          <label className="checkbox-inline">
            <input
              type="checkbox"
              checked={keepStructure}
              onChange={(e) => onKeepStructureChanged(e.target.checked)}
              disabled={busy || disabled}
            />
            Keep structure (ZIP)
          </label>
        )}
      </div>

      <div
        className="drop-zone"
        onDragOver={(e) => e.preventDefault()}
        onDrop={onDrop}
        role="button"
        tabIndex={0}
      >
        Drag & drop files or folders here
      </div>

      <p className="mini-text">{sources.length} sources selected - {formatBytes(totalBytes)}</p>
      {feedback && <p className="feedback error">{feedback}</p>}

      <ul className="source-list">
        {sources.map((source) => (
          <li key={source.id}>
            <div>
              <strong>{source.kind === 'folder' ? 'Folder' : 'File'}</strong>
              <p>{source.path}</p>
            </div>
            <button
              type="button"
              className="ghost"
              onClick={() => void remove(source.id)}
              disabled={busy || disabled}
            >
              Remove
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

function normalizeSelection(selection: string | string[] | null): string[] {
  if (!selection) {
    return [];
  }
  return Array.isArray(selection) ? selection : [selection];
}
