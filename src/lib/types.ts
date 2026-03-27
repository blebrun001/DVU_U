export type SessionState =
  | 'draft'
  | 'scanning'
  | 'analyzing'
  | 'ready'
  | 'uploading'
  | 'paused'
  | 'cancelling'
  | 'completed'
  | 'completed_with_errors'
  | 'failed'
  | 'interrupted';

export type ItemState =
  | 'pending_scan'
  | 'ignored'
  | 'ready'
  | 'uploading'
  | 'uploaded'
  | 'skipped_existing'
  | 'conflict'
  | 'retrying'
  | 'error'
  | 'cancelled';

export interface DestinationConfigInput {
  serverUrl: string;
  datasetPid: string;
  apiToken: string;
}

export interface AnalyzeBatchInput {
  keepStructure?: boolean;
}

export interface AnalysisProgressEvent {
  step: number;
  totalSteps: number;
  message: string;
}

export interface RecentDatasetsInput {
  serverUrl: string;
  apiToken: string;
}

export interface RecentDatasetOption {
  persistentId: string;
  title: string;
}

export type DestinationErrorKind =
  | 'network'
  | 'auth'
  | 'dataset_not_found'
  | 'permission'
  | 'invalid_input'
  | 'unknown';

export interface DestinationValidationResult {
  ok: boolean;
  normalizedServerUrl?: string;
  datasetTitle?: string;
  datasetId?: number;
  uploadSupported?: boolean;
  directUploadSupported?: boolean;
  errorKind?: DestinationErrorKind;
  message?: string;
}

export interface SourceEntry {
  id: string;
  path: string;
  kind: 'file' | 'folder';
  recursive: boolean;
  addedAt: string;
}

export interface ScanSummary {
  totalFiles: number;
  totalBytes: number;
  unreadableCount: number;
  ignoredSymlinkCount: number;
  duplicatePathCount: number;
}

export interface AnalysisItemDecision {
  itemId: string;
  localPath: string;
  relativePath: string;
  fileName: string;
  sizeBytes: number;
  checksumSha256?: string;
  decision: 'ready' | 'skip_existing' | 'conflict' | 'ignored' | 'error';
  reason?: string;
}

export interface AnalysisSummary {
  totalFiles: number;
  totalBytes: number;
  toUploadFiles: number;
  toUploadBytes: number;
  skippedExistingFiles: number;
  conflictFiles: number;
  ignoredFiles: number;
  errorFiles: number;
  blockingErrors: string[];
}

export interface TransferPlan {
  sessionId: string;
  createdAt: string;
  summary: AnalysisSummary;
  items: AnalysisItemDecision[];
}

export interface FileTransferProgress {
  itemId: string;
  fileName: string;
  state: ItemState;
  uploadedBytes: number;
  totalBytes: number;
  attempt: number;
  message?: string;
}

export interface TransferSnapshot {
  sessionId: string;
  state: SessionState;
  startedAt?: string;
  updatedAt: string;
  totalBytes: number;
  uploadedBytes: number;
  throughputBytesPerSec: number;
  etaSeconds?: number;
  completedFiles: number;
  totalFiles: number;
  errorFiles: number;
  retryingFiles: number;
  activeFile?: FileTransferProgress;
  lastMessage?: string;
}

export type TransferControlAction = 'start' | 'pause' | 'resume' | 'cancel';

export interface FinalReport {
  sessionId: string;
  startedAt?: string;
  finishedAt?: string;
  durationSeconds?: number;
  totalFiles: number;
  uploadedFiles: number;
  skippedFiles: number;
  conflictFiles: number;
  errorFiles: number;
  cancelledFiles: number;
  totalBytes: number;
  uploadedBytes: number;
  entries: Array<{
    itemId: string;
    fileName: string;
    localPath: string;
    state: ItemState;
    bytesUploaded: number;
    totalBytes: number;
    message?: string;
  }>;
}

export interface HistoryEntry {
  sessionId: string;
  datasetPid: string;
  serverUrl: string;
  state: SessionState;
  startedAt?: string;
  finishedAt?: string;
  totalFiles: number;
  uploadedFiles: number;
  errorFiles: number;
  totalBytes: number;
}

export interface BootstrapState {
  sessionId: string;
  sessionState: SessionState;
  destination?: DestinationBootstrap;
  sources: SourceEntry[];
  scanSummary?: ScanSummary;
  transferPlan?: TransferPlan;
  lastSnapshot?: TransferSnapshot;
  finalReport?: FinalReport;
}

export interface DestinationBootstrap {
  serverUrl: string;
  datasetPid: string;
  hasToken: boolean;
}

export interface OperationResult {
  ok: boolean;
  message?: string;
}
