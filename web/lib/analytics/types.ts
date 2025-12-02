/**
 * Analytics Types & Interfaces
 *
 * Centralized type definitions for analytics events and parameters.
 */

/**
 * Analytics event names - centralized for type safety and consistency
 */
export type AnalyticsEventName =
  // Authentication events
  | "user_signed_in"
  | "user_signed_out"
  | "sign_in_attempted"
  | "sign_in_failed"
  // Video processing events
  | "video_processing_started"
  | "video_processing_completed"
  | "video_processing_failed"
  | "video_processing_cancelled"
  // Clip events
  | "clip_downloaded"
  | "clip_copied_link"
  | "clip_published_tiktok"
  | "clip_published_failed"
  // Navigation events
  | "page_view"
  | "navigation_clicked"
  // Engagement events
  | "cta_clicked"
  | "feature_viewed"
  | "error_encountered";

/**
 * Event parameter names - standardized for consistency
 */
export interface AnalyticsEventParams {
  // Common parameters
  page_name?: string;
  page_path?: string;
  page_title?: string;
  // Video processing
  video_id?: string;
  video_url?: string;
  style?: string;
  has_custom_prompt?: boolean;
  processing_duration_ms?: number;
  clips_generated?: number;
  // Clip parameters
  clip_id?: string;
  clip_name?: string;
  clip_style?: string;
  // Error parameters
  error_type?: string;
  error_message?: string;
  error_code?: string;
  // User engagement
  cta_name?: string;
  feature_name?: string;
  // TikTok publishing
  tiktok_publish_success?: boolean;
  tiktok_error_type?: string;
}

/**
 * Analytics configuration
 */
export interface AnalyticsConfig {
  enabled: boolean;
  debug: boolean;
  respectDoNotTrack: boolean;
  maxEventQueueSize: number;
  batchIntervalMs: number;
}
