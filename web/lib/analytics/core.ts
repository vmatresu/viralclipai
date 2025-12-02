/**
 * Core Analytics Functions
 * 
 * Initialization, event tracking, and user management for Firebase Analytics.
 */

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
import { AnalyticsConfig, AnalyticsEventParams, AnalyticsEventName } from "./types";
import { DEFAULT_CONFIG } from "./config";
import { shouldEnableAnalytics, sanitizeEventName, sanitizeParams, validateEvent } from "./utils";

// ============================================================================
// State Management
// ============================================================================

let analyticsInstance: Analytics | null = null;
let initializationPromise: Promise<Analytics | null> | null = null;
let isInitialized = false;
let config: AnalyticsConfig = { ...DEFAULT_CONFIG };
const eventQueue: Array<{ eventName: string; params?: AnalyticsEventParams }> = [];

// ============================================================================
// Initialization
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
      if (!shouldEnableAnalytics(config)) {
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
      if (!shouldEnableAnalytics(config)) {
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

// ============================================================================
// Event Tracking
// ============================================================================

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
  if (!shouldEnableAnalytics(config)) {
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

// ============================================================================
// User Management
// ============================================================================

/**
 * Set user ID for analytics
 */
export async function setAnalyticsUserId(userId: string | null): Promise<void> {
  if (!shouldEnableAnalytics(config) || typeof window === "undefined") {
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
  if (!shouldEnableAnalytics(config) || typeof window === "undefined") {
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

// ============================================================================
// Configuration & State
// ============================================================================

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

