import { invoke } from '@tauri-apps/api/core';
import type {
  AnalysisSummary,
  AnalyzeBatchInput,
  BootstrapState,
  DestinationConfigInput,
  RecentDatasetsInput,
  RecentDatasetOption,
  DestinationValidationResult,
  FinalReport,
  HistoryEntry,
  OperationResult,
  ScanSummary,
  SourceEntry,
  TransferPlan,
  TransferSnapshot
} from './types';

export async function loadBootstrapState(): Promise<BootstrapState> {
  return invoke<BootstrapState>('load_bootstrap_state');
}

export async function saveDestination(input: DestinationConfigInput): Promise<OperationResult> {
  return invoke<OperationResult>('save_destination', { input });
}

export async function testDestination(
  input: DestinationConfigInput
): Promise<DestinationValidationResult> {
  return invoke<DestinationValidationResult>('test_destination', { input });
}

export async function listRecentDatasets(
  input: RecentDatasetsInput
): Promise<RecentDatasetOption[]> {
  if (!input.serverUrl.trim()) {
    return [];
  }
  return invoke<RecentDatasetOption[]>('list_recent_datasets', { input });
}

export async function addSources(paths: string[], recursive: boolean): Promise<SourceEntry[]> {
  return invoke<SourceEntry[]>('add_sources', { paths, recursive });
}

export async function removeSource(sourceId: string): Promise<OperationResult> {
  return invoke<OperationResult>('remove_source', { sourceId });
}

export async function scanSources(): Promise<ScanSummary> {
  return invoke<ScanSummary>('scan_sources');
}

export async function analyzeBatch(input: AnalyzeBatchInput = {}): Promise<TransferPlan> {
  return invoke<TransferPlan>('analyze_batch', { input });
}

export async function startTransfer(): Promise<OperationResult> {
  return invoke<OperationResult>('start_transfer');
}

export async function pauseTransfer(): Promise<OperationResult> {
  return invoke<OperationResult>('pause_transfer');
}

export async function resumeTransfer(): Promise<OperationResult> {
  return invoke<OperationResult>('resume_transfer');
}

export async function cancelTransfer(): Promise<OperationResult> {
  return invoke<OperationResult>('cancel_transfer');
}

export async function getTransferSnapshot(): Promise<TransferSnapshot | null> {
  return invoke<TransferSnapshot | null>('get_transfer_snapshot');
}

export async function getAnalysisSummary(): Promise<AnalysisSummary | null> {
  return invoke<AnalysisSummary | null>('get_analysis_summary');
}

export async function getFinalReport(): Promise<FinalReport | null> {
  return invoke<FinalReport | null>('get_final_report');
}

export async function listHistory(): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>('list_history');
}

export async function restoreLastInterrupted(): Promise<OperationResult> {
  return invoke<OperationResult>('restore_last_interrupted');
}

export async function exportReport(format: 'json' | 'csv'): Promise<OperationResult> {
  return invoke<OperationResult>('export_report', { format });
}
