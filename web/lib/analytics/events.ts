/**
 * Predefined Analytics Events
 * 
 * Type-safe event helpers for common analytics events.
 */

import { trackEvent, setAnalyticsUserId } from "./core";
import { AnalyticsEventParams } from "./types";

/**
 * Predefined analytics events with type safety
 */
export const analyticsEvents = {
  // Authentication
  userSignedIn: (userId?: string) => {
    trackEvent("user_signed_in");
    if (userId) {
      setAnalyticsUserId(userId);
    }
  },

  userSignedOut: () => {
    trackEvent("user_signed_out");
    setAnalyticsUserId(null);
  },

  signInAttempted: () => trackEvent("sign_in_attempted"),

  signInFailed: (reason?: string) =>
    trackEvent("sign_in_failed", { error_message: reason }),

  // Video Processing
  videoProcessingStarted: (params: {
    style: string;
    hasCustomPrompt: boolean;
    videoUrl?: string;
  }) =>
    trackEvent("video_processing_started", {
      style: params.style,
      has_custom_prompt: params.hasCustomPrompt,
      video_url: params.videoUrl,
    }),

  videoProcessingCompleted: (params: {
    videoId: string;
    style: string;
    clipsGenerated: number;
    durationMs: number;
    hasCustomPrompt: boolean;
  }) =>
    trackEvent("video_processing_completed", {
      video_id: params.videoId,
      style: params.style,
      clips_generated: params.clipsGenerated,
      processing_duration_ms: params.durationMs,
      has_custom_prompt: params.hasCustomPrompt,
    }),

  videoProcessingFailed: (params: {
    errorType: string;
    errorMessage: string;
    style?: string;
  }) =>
    trackEvent("video_processing_failed", {
      error_type: params.errorType,
      error_message: params.errorMessage,
      style: params.style,
    }),

  videoProcessingCancelled: () => trackEvent("video_processing_cancelled"),

  // Clips
  clipDownloaded: (params: { clipId: string; clipName: string; style: string }) =>
    trackEvent("clip_downloaded", {
      clip_id: params.clipId,
      clip_name: params.clipName,
      clip_style: params.style,
    }),

  clipCopiedLink: (params: { clipId: string; clipName: string }) =>
    trackEvent("clip_copied_link", {
      clip_id: params.clipId,
      clip_name: params.clipName,
    }),

  clipPublishedTikTok: (params: { clipId: string; clipName: string; success: boolean }) =>
    trackEvent("clip_published_tiktok", {
      clip_id: params.clipId,
      clip_name: params.clipName,
      tiktok_publish_success: params.success,
    }),

  clipPublishedFailed: (params: {
    clipId: string;
    clipName: string;
    errorType: string;
  }) =>
    trackEvent("clip_published_failed", {
      clip_id: params.clipId,
      clip_name: params.clipName,
      tiktok_error_type: params.errorType,
    }),

  // Navigation
  pageViewed: (params: {
    pageName: string;
    pagePath?: string;
    pageTitle?: string;
  }) =>
    trackEvent("page_view", {
      page_name: params.pageName,
      page_path: params.pagePath,
      page_title: params.pageTitle,
    }),

  navigationClicked: (params: { destination: string; source?: string }) =>
    trackEvent("navigation_clicked", {
      page_name: params.destination,
      feature_name: params.source,
    }),

  // Engagement
  ctaClicked: (params: { ctaName: string; location?: string }) =>
    trackEvent("cta_clicked", {
      cta_name: params.ctaName,
      page_name: params.location,
    }),

  featureViewed: (params: { featureName: string; pageName?: string }) =>
    trackEvent("feature_viewed", {
      feature_name: params.featureName,
      page_name: params.pageName,
    }),

  // Errors
  errorEncountered: (params: {
    errorType: string;
    errorMessage: string;
    errorCode?: string;
    pageName?: string;
  }) =>
    trackEvent("error_encountered", {
      error_type: params.errorType,
      error_message: params.errorMessage,
      error_code: params.errorCode,
      page_name: params.pageName,
    }),
};

