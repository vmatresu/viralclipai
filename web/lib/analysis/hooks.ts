/**
 * Analysis SWR Hooks
 * React hooks for fetching and mutating analysis data.
 */

import { useAuth } from '@/lib/auth';
import useSWR from 'swr';
import useSWRMutation from 'swr/mutation';
import * as api from './api';
import type {
    AnalysisStatusResponse,
    DraftWithScenesResponse,
    ListDraftsResponse,
    ProcessDraftRequest,
    ProcessDraftResponse,
    ProcessingEstimate,
    StartAnalysisRequest,
    StartAnalysisResponse,
} from './types';

/**
 * Hook for starting a new analysis.
 */
export function useStartAnalysis() {
  const { getIdToken } = useAuth();

  return useSWRMutation<StartAnalysisResponse, Error, string, StartAnalysisRequest>(
    'start-analysis',
    async (_key, { arg }) => {
      const token = await getIdToken();
      if (!token) throw new Error('Authentication required');
      return api.startAnalysis(arg, token);
    }
  );
}

/**
 * Hook for polling analysis status.
 * Automatically refreshes every 2 seconds while analysis is in progress.
 */
export function useAnalysisStatus(draftId: string | null, enabled = true) {
  const { getIdToken } = useAuth();

  const { data, error, isLoading, mutate } = useSWR<AnalysisStatusResponse>(
    enabled && draftId ? [`analysis-status`, draftId] : null,
    async () => {
      const token = await getIdToken();
      if (!token || !draftId) throw new Error('Missing token or draftId');
      return api.getAnalysisStatus(draftId, token);
    },
    {
      refreshInterval: (data) => {
        // Refresh every 2 seconds while analysis is in progress
        if (data && ['pending', 'downloading', 'analyzing'].includes(data.status)) {
          return 2000;
        }
        return 0; // Stop refreshing when complete
      },
      revalidateOnFocus: false,
    }
  );

  return {
    status: data,
    error,
    isLoading,
    refresh: mutate,
  };
}

/**
 * Hook for listing user's drafts.
 */
export function useDrafts() {
  const { getIdToken, user } = useAuth();

  const { data, error, isLoading, mutate } = useSWR<ListDraftsResponse>(
    user ? 'drafts' : null,
    async () => {
      const token = await getIdToken();
      if (!token) throw new Error('Authentication required');
      return api.listDrafts(token);
    }
  );

  return {
    drafts: data?.drafts ?? [],
    error,
    isLoading,
    refresh: mutate,
  };
}

/**
 * Hook for getting a single draft with scenes.
 */
export function useDraft(draftId: string | null) {
  const { getIdToken, user } = useAuth();

  const { data, error, isLoading, mutate } = useSWR<DraftWithScenesResponse>(
    draftId && user ? [`draft`, draftId] : null,
    async () => {
      const token = await getIdToken();
      if (!token || !draftId) throw new Error('Missing token or draftId');
      return api.getDraft(draftId, token);
    }
  );

  return {
    draft: data?.draft,
    scenes: data?.scenes ?? [],
    warnings: data?.warnings ?? [],
    error,
    isLoading,
    refresh: mutate,
  };
}

/**
 * Hook for deleting a draft.
 */
export function useDeleteDraft() {
  const { getIdToken } = useAuth();

  return useSWRMutation<void, Error, string, string>(
    'delete-draft',
    async (_key, { arg: draftId }) => {
      const token = await getIdToken();
      if (!token) throw new Error('Authentication required');
      await api.deleteDraft(draftId, token);
    }
  );
}

/**
 * Hook for processing a draft.
 */
export function useProcessDraft() {
  const { getIdToken } = useAuth();

  return useSWRMutation<
    ProcessDraftResponse,
    Error,
    string,
    { draftId: string; request: ProcessDraftRequest }
  >(
    'process-draft',
    async (_key, { arg }) => {
      const token = await getIdToken();
      if (!token) throw new Error('Authentication required');
      return api.processDraft(arg.draftId, arg.request, token);
    }
  );
}

/**
 * Hook for getting processing estimates.
 */
export function useProcessingEstimate(
  draftId: string | null,
  sceneIds: number[],
  fullCount: number,
  splitCount: number
) {
  const { getIdToken, user } = useAuth();

  const shouldFetch = 
    draftId && 
    user && 
    sceneIds.length > 0 && 
    (fullCount > 0 || splitCount > 0);

  const { data, error, isLoading } = useSWR<ProcessingEstimate>(
    shouldFetch
      ? [`estimate`, draftId, sceneIds.join(','), fullCount, splitCount]
      : null,
    async () => {
      const token = await getIdToken();
      if (!token || !draftId) throw new Error('Missing token or draftId');
      return api.getProcessingEstimate(draftId, sceneIds, fullCount, splitCount, token);
    },
    {
      dedupingInterval: 1000, // Debounce rapid changes
    }
  );

  return {
    estimate: data,
    error,
    isLoading,
  };
}
