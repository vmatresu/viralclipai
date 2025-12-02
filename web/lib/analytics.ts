"use client";

import {
  getAnalytics,
  Analytics,
  logEvent,
  isSupported,
  setAnalyticsCollectionEnabled,
  setUserId,
  setUserProperties,
} from "firebase/analytics";
import { getApps } from "firebase/app";
import { frontendLogger } from "@/lib/logger";

// ============================================================================
// Types & Interfaces
// ============================================================================

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
interface AnalyticsConfig {
  enabled: boolean;
  debug: boolean;
  respectDoNotTrack: boolean;
  maxEventQueueSize: number;
  batchIntervalMs: number;
}

// ============================================================================
// Constants & Configuration
// ============================================================================

const DEFAULT_CONFIG: AnalyticsConfig = {
  enabled: true,
  debug: process.env.NODE_ENV === "development",
  respectDoNotTrack: true,
  maxEventQueueSize: 50,
  batchIntervalMs: 1000,
};

// Firebase Analytics has limits on event names and parameters
const MAX_EVENT_NAME_LENGTH = 40;
const MAX_PARAM_NAME_LENGTH = 40;
const MAX_PARAM_VALUE_LENGTH = 100;
const MAX_PARAMS_PER_EVENT = 25;

// ============================================================================
// State Management
// ============================================================================

let analyticsInstance: Analytics | null = null;
let initializationPromise: Promise<Analytics | null> | null = null;
let isInitialized = false;
let config: AnalyticsConfig = { ...DEFAULT_CONFIG };
const eventQueue: Array<{ eventName: string; params?: AnalyticsEventParams }> = [];

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Check if analytics should be enabled based on privacy settings
 */
function shouldEnableAnalytics(): boolean {
  if (!config.enabled) {
    return false;
  }

  if (typeof window === "undefined") {
    return false;
  }

  // Respect Do Not Track header
  if (config.respectDoNotTrack && navigator.doNotTrack === "1") {
    frontendLogger.info("Analytics disabled: Do Not Track enabled");
    return false;
  }

  // Check for privacy consent (can be extended with cookie consent)
  const privacyConsent = localStorage.getItem("analytics_consent");
  if (privacyConsent === "false") {
    return false;
  }

  return true;
}

/**
 * Sanitize event name to meet Firebase requirements
 */
function sanitizeEventName(eventName: string): string {
  // Firebase requires: alphanumeric and underscores only, max 40 chars
  let sanitized = eventName
    .replace(/[^a-zA-Z0-9_]/g, "_")
    .substring(0, MAX_EVENT_NAME_LENGTH);

  // Ensure it doesn't start with a number
  if (/^\d/.test(sanitized)) {
    sanitized = `event_${sanitized}`;
  }

  return sanitized;
}

/**
 * Sanitize event parameters to meet Firebase requirements
 */
function sanitizeParams(params?: AnalyticsEventParams): Record<string, any> | undefined {
  if (!params) {
    return undefined;
  }

  const sanitized: Record<string, any> = {};
  let paramCount = 0;

  for (const [key, value] of Object.entries(params)) {
    if (paramCount >= MAX_PARAMS_PER_EVENT) {
      frontendLogger.warn(`Max params per event reached, truncating: ${key}`);
      break;
    }

    // Sanitize key
    let sanitizedKey = key
      .replace(/[^a-zA-Z0-9_]/g, "_")
      .substring(0, MAX_PARAM_NAME_LENGTH);

    // Ensure key doesn't start with a number
    if (/^\d/.test(sanitizedKey)) {
      sanitizedKey = `param_${sanitizedKey}`;
    }

    // Sanitize value
    let sanitizedValue: any = value;
    if (typeof value === "string") {
      sanitizedValue = value.substring(0, MAX_PARAM_VALUE_LENGTH);
    } else if (typeof value === "number") {
      // Firebase accepts numbers
      sanitizedValue = value;
    } else if (typeof value === "boolean") {
      sanitizedValue = value;
    } else {
      // Convert other types to string
      sanitizedValue = String(value).substring(0, MAX_PARAM_VALUE_LENGTH);
    }

    sanitized[sanitizedKey] = sanitizedValue;
    paramCount++;
  }

  return Object.keys(sanitized).length > 0 ? sanitized : undefined;
}

/**
 * Validate event before sending
 */
function validateEvent(eventName: string, params?: AnalyticsEventParams): boolean {
  if (!eventName || typeof eventName !== "string") {
    frontendLogger.error("Invalid event name:", eventName);
    return false;
  }

  if (eventName.length > MAX_EVENT_NAME_LENGTH) {
    frontendLogger.warn(`Event name too long: ${eventName}`);
    return false;
  }

  return true;
}

// ============================================================================
// Core Analytics Functions
// ============================================================================

/**
 * Initialize Firebase Analytics with proper error handling and checks
 */
export async function initAnalytics(): Promise<Analytics | null> {
  // Return existing instance if already initialized
  if (analyticsInstance) {
    return analyticsInstance;
  }

  // Return existing promise if initialization is in progress
  if (initializationPromise) {
    return initializationPromise;
  }

  // Create initialization promise
  initializationPromise = (async () => {
    try {
      // Browser environment check
      if (typeof window === "undefined") {
        return null;
      }

      // Privacy checks
      if (!shouldEnableAnalytics()) {
        frontendLogger.info("Analytics disabled by configuration or privacy settings");
        return null;
      }

      // Check if Firebase app is initialized
      if (!getApps().length) {
        frontendLogger.warn("Firebase app not initialized. Analytics will not work.");
        return null;
      }

      // Check if analytics is supported
      const supported = await isSupported();
      if (!supported) {
        frontendLogger.warn("Firebase Analytics is not supported in this environment.");
        return null;
      }

      // Check if measurement ID is configured
      const measurementId = process.env.NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID;
      if (!measurementId) {
        if (config.debug) {
          frontendLogger.info("Firebase Analytics measurement ID not configured. Analytics disabled.");
        }
        return null;
      }

      // Initialize analytics
      analyticsInstance = getAnalytics();
      isInitialized = true;

      // Disable analytics collection if user opted out
      if (!shouldEnableAnalytics()) {
        await setAnalyticsCollectionEnabled(analyticsInstance, false);
      }

      if (config.debug) {
        frontendLogger.info("Firebase Analytics initialized successfully");
      }

      // Process queued events
      processEventQueue();

      return analyticsInstance;
    } catch (error) {
      frontendLogger.error("Failed to initialize Firebase Analytics", error);
      analyticsInstance = null;
      isInitialized = false;
      return null;
    } finally {
      initializationPromise = null;
    }
  })();

  return initializationPromise;
}

/**
 * Process queued events after initialization
 */
function processEventQueue(): void {
  if (eventQueue.length === 0 || !analyticsInstance) {
    return;
  }

  const eventsToProcess = [...eventQueue];
  eventQueue.length = 0;

  eventsToProcess.forEach(({ eventName, params }) => {
    trackEventInternal(eventName, params);
  });
}

/**
 * Internal event tracking (assumes analytics is initialized)
 */
function trackEventInternal(
  eventName: string,
  params?: AnalyticsEventParams
): void {
  if (!analyticsInstance) {
    return;
  }

  try {
    const sanitizedName = sanitizeEventName(eventName);
    const sanitizedParams = sanitizeParams(params);

    logEvent(analyticsInstance, sanitizedName, sanitizedParams);

    if (config.debug) {
      frontendLogger.info(`Analytics event: ${sanitizedName}`, sanitizedParams);
    }
  } catch (error) {
    frontendLogger.error(`Failed to log analytics event: ${eventName}`, error);
  }
}

/**
 * Track an analytics event with validation and error handling
 */
export async function trackEvent(
  eventName: AnalyticsEventName | string,
  eventParams?: AnalyticsEventParams
): Promise<void> {
  // Browser environment check
  if (typeof window === "undefined") {
    return;
  }

  // Privacy check
  if (!shouldEnableAnalytics()) {
    return;
  }

  // Validate event
  if (!validateEvent(eventName, eventParams)) {
    return;
  }

  // Initialize if needed
  if (!isInitialized && !initializationPromise) {
    await initAnalytics();
  }

  // If still not initialized, queue the event
  if (!isInitialized) {
    if (eventQueue.length < config.maxEventQueueSize) {
      eventQueue.push({ eventName, params: eventParams });
    } else {
      frontendLogger.warn("Event queue full, dropping event:", eventName);
    }
    return;
  }

  // Track immediately if initialized
  trackEventInternal(eventName, eventParams);
}

/**
 * Set user ID for analytics
 */
export async function setAnalyticsUserId(userId: string | null): Promise<void> {
  if (!shouldEnableAnalytics() || typeof window === "undefined") {
    return;
  }

  if (!analyticsInstance) {
    await initAnalytics();
  }

  if (!analyticsInstance) {
    return;
  }

  try {
    if (userId) {
      await setUserId(analyticsInstance, userId);
    } else {
      await setUserId(analyticsInstance, null);
    }
  } catch (error) {
    frontendLogger.error("Failed to set analytics user ID", error);
  }
}

/**
 * Set user properties for analytics
 */
export async function setAnalyticsUserProperties(
  properties: Record<string, string>
): Promise<void> {
  if (!shouldEnableAnalytics() || typeof window === "undefined") {
    return;
  }

  if (!analyticsInstance) {
    await initAnalytics();
  }

  if (!analyticsInstance) {
    return;
  }

  try {
    await setUserProperties(analyticsInstance, properties);
  } catch (error) {
    frontendLogger.error("Failed to set analytics user properties", error);
  }
}

/**
 * Enable or disable analytics collection
 */
export async function setAnalyticsEnabled(enabled: boolean): Promise<void> {
  config.enabled = enabled;

  if (!analyticsInstance) {
    return;
  }

  try {
    await setAnalyticsCollectionEnabled(analyticsInstance, enabled);
    localStorage.setItem("analytics_consent", String(enabled));
  } catch (error) {
    frontendLogger.error("Failed to set analytics collection enabled", error);
  }
}

/**
 * Get analytics instance (for advanced usage)
 */
export function getAnalyticsInstance(): Analytics | null {
  return analyticsInstance;
}

/**
 * Check if analytics is initialized
 */
export function isAnalyticsInitialized(): boolean {
  return isInitialized && analyticsInstance !== null;
}

// ============================================================================
// Predefined Event Helpers
// ============================================================================

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
