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
  const normalized = input.serverUrl.trim().replace(/\/+$/, '');
  if (!normalized) {
    return [];
  }

  const parsed = new URL(normalized);
  const pathParts = parsed.pathname.split('/').filter(Boolean);
  const subtree = pathParts[0]?.toLowerCase() === 'dataverse' && pathParts[1] ? pathParts[1] : '';

  const candidates = [normalized];
  const origin = parsed.origin.replace(/\/+$/, '');
  if (!candidates.includes(origin)) {
    candidates.push(origin);
  }

  let lastError: Error | null = null;
  for (const baseUrl of candidates) {
    try {
      const url = new URL(`${baseUrl}/api/search`);
      url.searchParams.set('q', '*');
      url.searchParams.set('type', 'dataset');
      url.searchParams.set('sort', 'date');
      url.searchParams.set('order', 'desc');
      url.searchParams.set('per_page', '10');
      if (subtree) {
        url.searchParams.set('subtree', subtree);
      }

      const headers: Record<string, string> = {
        Accept: 'application/json'
      };
      const token = input.apiToken.trim();
      if (token) {
        headers['X-Dataverse-Key'] = token;
      }

      const response = await fetch(url.toString(), { method: 'GET', headers });
      if (!response.ok) {
        if (response.status === 404) {
          continue;
        }
        throw new Error(`Dataverse recent dataset lookup failed (HTTP ${response.status})`);
      }

      const payload = (await response.json()) as {
        data?: { items?: Array<Record<string, unknown>> };
      };
      const items = payload.data?.items ?? [];

      return items
        .map((entry) => {
          const persistentId = String(entry.global_id ?? entry.globalId ?? '').trim();
          if (!persistentId) {
            return null;
          }
          const title = String(entry.name ?? entry.title ?? persistentId).trim();
          return {
            persistentId,
            title
          };
        })
        .filter((value): value is RecentDatasetOption => value !== null);
    } catch (err) {
      lastError = err instanceof Error ? err : new Error(String(err));
    }
  }

  throw lastError ?? new Error('Dataverse recent dataset lookup failed.');
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
