/**
 * Analysis API Client
 * API functions for the two-step video analysis workflow.
 */

import { apiFetch } from '@/lib/apiClient';
import type {
    AnalysisStatusResponse,
    DeleteDraftResponse,
    DraftWithScenesResponse,
    ListDraftsResponse,
    ProcessDraftRequest,
    ProcessDraftResponse,
    ProcessingEstimate,
    StartAnalysisRequest,
    StartAnalysisResponse,
} from './types';

/**
 * Start a new video analysis.
 * Returns a job_id for polling and draft_id for later access.
 */
export async function startAnalysis(
  request: StartAnalysisRequest,
  token: string
): Promise<StartAnalysisResponse> {
  return apiFetch<StartAnalysisResponse>('/api/analyze', {
    method: 'POST',
    token,
    body: request,
  });
}

/**
 * Get the status of an analysis job.
 */
export async function getAnalysisStatus(
  draftId: string,
  token: string
): Promise<AnalysisStatusResponse> {
  return apiFetch<AnalysisStatusResponse>(
    `/api/analyze/${encodeURIComponent(draftId)}/status`,
    {
      method: 'GET',
      token,
    }
  );
}

/**
 * List all drafts for the current user.
 */
export async function listDrafts(token: string): Promise<ListDraftsResponse> {
  return apiFetch<ListDraftsResponse>('/api/drafts', {
    method: 'GET',
    token,
  });
}

/**
 * Get a draft with all its scenes.
 */
export async function getDraft(
  draftId: string,
  token: string
): Promise<DraftWithScenesResponse> {
  return apiFetch<DraftWithScenesResponse>(
    `/api/drafts/${encodeURIComponent(draftId)}`,
    {
      method: 'GET',
      token,
    }
  );
}

/**
 * Delete a draft.
 */
export async function deleteDraft(
  draftId: string,
  token: string
): Promise<DeleteDraftResponse> {
  return apiFetch<DeleteDraftResponse>(
    `/api/drafts/${encodeURIComponent(draftId)}`,
    {
      method: 'DELETE',
      token,
    }
  );
}

/**
 * Submit selected scenes from a draft for processing.
 * Auto-generates an idempotency key if not provided.
 */
export async function processDraft(
  draftId: string,
  request: ProcessDraftRequest,
  token: string
): Promise<ProcessDraftResponse> {
  // Auto-generate idempotency key if not provided
  const idempotencyKey = request.idempotency_key || crypto.randomUUID();
  
  return apiFetch<ProcessDraftResponse>(
    `/api/drafts/${encodeURIComponent(draftId)}/process`,
    {
      method: 'POST',
      token,
      body: {
        ...request,
        idempotency_key: idempotencyKey,
      },
    }
  );
}

/**
 * Get cost/time estimates for processing.
 */
export async function getProcessingEstimate(
  draftId: string,
  sceneIds: number[],
  fullCount: number,
  splitCount: number,
  token: string
): Promise<ProcessingEstimate> {
  const params = new URLSearchParams({
    scene_ids: sceneIds.join(','),
    full_count: fullCount.toString(),
    split_count: splitCount.toString(),
  });

  return apiFetch<ProcessingEstimate>(
    `/api/drafts/${encodeURIComponent(draftId)}/estimate?${params}`,
    {
      method: 'GET',
      token,
    }
  );
}
