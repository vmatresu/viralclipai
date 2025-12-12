/**
 * Zod validation schemas for analysis API responses.
 *
 * Validates API responses at runtime to catch schema drift early
 * and fail gracefully with clear error messages.
 */

import { z } from "zod";

// ============================================================================
// Base Schemas
// ============================================================================

/** Analysis status enum */
export const AnalysisStatusSchema = z.enum([
  "pending",
  "downloading",
  "analyzing",
  "completed",
  "failed",
  "expired",
]);

/** Scene selection for processing */
export const SceneSelectionSchema = z.object({
  scene_id: z.number().int().positive(),
  render_full: z.boolean(),
  render_split: z.boolean(),
});

// ============================================================================
// Draft Schemas
// ============================================================================

/** Analysis draft (video that has been analyzed) */
export const AnalysisDraftSchema = z.object({
  id: z.string().min(1),
  source_url: z.string().url(),
  video_title: z.string().optional().nullable(),
  prompt_instructions: z.string().optional().nullable(),
  status: AnalysisStatusSchema,
  error_message: z.string().optional().nullable(),
  request_id: z.string().optional().nullable(),
  scene_count: z.number().int().min(0),
  warning_count: z.number().int().min(0),
  created_at: z.string(), // ISO datetime
  updated_at: z.string().optional(), // ISO datetime
  expires_at: z.string(), // ISO datetime
});

/** Draft scene (a detected highlight) */
export const DraftSceneSchema = z.object({
  id: z.number().int().positive(),
  analysis_draft_id: z.string().optional(), // May not be returned in list
  title: z.string(),
  description: z.string().optional().nullable(),
  reason: z.string().optional().nullable(),
  start: z.string(), // HH:MM:SS format
  end: z.string(), // HH:MM:SS format
  duration_secs: z.number().int().min(0),
  pad_before: z.number().min(0).default(1.0),
  pad_after: z.number().min(0).default(1.0),
  confidence: z.number().min(0).max(1).optional().nullable(),
  hook_category: z.string().optional().nullable(),
});

// ============================================================================
// API Response Schemas
// ============================================================================

/** Response from POST /api/analyze */
export const StartAnalysisResponseSchema = z.object({
  job_id: z.string().min(1),
  draft_id: z.string().min(1),
});

/** Response from GET /api/analyze/:draft_id/status */
export const AnalysisStatusResponseSchema = z.object({
  status: AnalysisStatusSchema,
  draft_id: z.string(),
  video_title: z.string().optional().nullable(),
  error_message: z.string().optional().nullable(),
  scene_count: z.number().int().min(0),
  warning_count: z.number().int().min(0),
});

/** Response from GET /api/drafts */
export const ListDraftsResponseSchema = z.object({
  drafts: z.array(AnalysisDraftSchema),
});

/** Response from GET /api/drafts/:draft_id */
export const DraftWithScenesResponseSchema = z.object({
  draft: AnalysisDraftSchema,
  scenes: z.array(DraftSceneSchema),
  warnings: z.array(z.string()).optional().default([]),
});

/** Response from DELETE /api/drafts/:draft_id */
export const DeleteDraftResponseSchema = z.object({
  success: z.boolean(),
  draft_id: z.string(),
});

/** Response from POST /api/drafts/:draft_id/process */
export const ProcessDraftResponseSchema = z.object({
  success: z.boolean(),
  draft_id: z.string(),
  video_id: z.string(),
  jobs_enqueued: z.number().int().min(0),
});

/** Response from GET /api/drafts/:draft_id/estimate */
export const ProcessingEstimateSchema = z.object({
  scene_count: z.number().int().min(0),
  total_duration_secs: z.number().int().min(0),
  estimated_credits: z.number().int().min(0),
  estimated_time_min_secs: z.number().int().min(0),
  estimated_time_max_secs: z.number().int().min(0),
  full_render_count: z.number().int().min(0),
  split_render_count: z.number().int().min(0),
  exceeds_quota: z.boolean(),
});

// ============================================================================
// Type Exports (infer from schemas)
// ============================================================================

export type AnalysisStatusType = z.infer<typeof AnalysisStatusSchema>;
export type SceneSelectionType = z.infer<typeof SceneSelectionSchema>;
export type AnalysisDraftType = z.infer<typeof AnalysisDraftSchema>;
export type DraftSceneType = z.infer<typeof DraftSceneSchema>;
export type StartAnalysisResponseType = z.infer<typeof StartAnalysisResponseSchema>;
export type AnalysisStatusResponseType = z.infer<typeof AnalysisStatusResponseSchema>;
export type ListDraftsResponseType = z.infer<typeof ListDraftsResponseSchema>;
export type DraftWithScenesResponseType = z.infer<typeof DraftWithScenesResponseSchema>;
export type DeleteDraftResponseType = z.infer<typeof DeleteDraftResponseSchema>;
export type ProcessDraftResponseType = z.infer<typeof ProcessDraftResponseSchema>;
export type ProcessingEstimateType = z.infer<typeof ProcessingEstimateSchema>;

// ============================================================================
// Validation Helpers
// ============================================================================

/**
 * Safely parse and validate an API response.
 * Returns the validated data or throws a human-readable error.
 */
export function validateResponse<T>(
  schema: z.ZodSchema<T>,
  data: unknown,
  context: string
): T {
  const result = schema.safeParse(data);
  if (!result.success) {
    const issues = result.error.issues
      .map((i) => `${i.path.join(".")}: ${i.message}`)
      .join(", ");
    console.error(`[Validation] ${context}: ${issues}`, data);
    throw new Error(`Invalid response from ${context}: ${issues}`);
  }
  return result.data;
}

/**
 * Safely parse with fallback - returns partial data if possible.
 * Logs warnings but doesn't throw.
 */
export function parseWithFallback<T>(
  schema: z.ZodSchema<T>,
  data: unknown,
  context: string
): Partial<T> | null {
  try {
    return schema.parse(data);
  } catch (error) {
    console.warn(`[Validation] Partial parse for ${context}:`, error);
    return data as Partial<T>;
  }
}
