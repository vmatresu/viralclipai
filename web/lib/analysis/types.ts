/**
 * Analysis Workflow Types
 * Types for the two-step video analysis and processing workflow.
 */

// ============================================================================
// Analysis Status
// ============================================================================

export type AnalysisStatus = 
  | 'pending'
  | 'downloading'
  | 'analyzing'
  | 'completed'
  | 'failed'
  | 'expired';

export function isTerminalStatus(status: AnalysisStatus): boolean {
  return status === 'completed' || status === 'failed' || status === 'expired';
}

export function isActiveStatus(status: AnalysisStatus): boolean {
  return status === 'pending' || status === 'downloading' || status === 'analyzing';
}

// ============================================================================
// Analysis Draft
// ============================================================================

export interface AnalysisDraft {
  id: string;
  user_id: string;
  source_url: string;
  video_title?: string;
  request_id?: string;
  prompt_used?: string;
  status: AnalysisStatus;
  error_message?: string;
  scene_count: number;
  warning_count: number;
  created_at: string;
  expires_at: string;
}

export interface DraftScene {
  id: number;
  analysis_draft_id: string;
  title: string;
  description?: string;
  reason?: string;
  start: string;
  end: string;
  duration_secs: number;
  pad_before: number;
  pad_after: number;
  confidence?: number;
  hook_category?: string;
}

// ============================================================================
// API Request/Response Types
// ============================================================================

export interface StartAnalysisRequest {
  url: string;
  prompt?: string;
}

export interface StartAnalysisResponse {
  job_id: string;
  draft_id: string;
}

export interface AnalysisStatusResponse {
  status: AnalysisStatus;
  draft_id: string;
  video_title?: string;
  error_message?: string;
  scene_count: number;
  warning_count: number;
}

export interface DraftSummary {
  id: string;
  source_url: string;
  video_title?: string;
  status: AnalysisStatus;
  scene_count: number;
  created_at: string;
  expires_at: string;
}

export interface ListDraftsResponse {
  drafts: DraftSummary[];
}

export interface DraftWithScenesResponse {
  draft: AnalysisDraft;
  scenes: DraftScene[];
  warnings?: string[];
}

export interface DeleteDraftResponse {
  success: boolean;
  draft_id: string;
}

export interface SceneSelection {
  scene_id: number;
  render_full: boolean;
  render_split: boolean;
}

export interface ProcessDraftRequest {
  analysis_draft_id: string;
  selected_scenes: SceneSelection[];
  full_style: string;
  split_style: string;
  idempotency_key?: string;
}

export interface ProcessDraftResponse {
  success: boolean;
  draft_id: string;
  video_id: string;
  jobs_enqueued: number;
}

export interface ProcessingEstimate {
  scene_count: number;
  total_duration_secs: number;
  estimated_credits: number;
  estimated_time_min_secs: number;
  estimated_time_max_secs: number;
  full_render_count: number;
  split_render_count: number;
  exceeds_quota: boolean;
}
