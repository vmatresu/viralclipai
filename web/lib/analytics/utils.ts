/**
 * Analytics Utility Functions
 * 
 * Sanitization, validation, and privacy utilities for analytics.
 */

import { frontendLogger } from "@/lib/logger";
import { AnalyticsConfig, AnalyticsEventParams } from "./types";
import {
  MAX_EVENT_NAME_LENGTH,
  MAX_PARAM_NAME_LENGTH,
  MAX_PARAM_VALUE_LENGTH,
  MAX_PARAMS_PER_EVENT,
} from "./config";

/**
 * Check if analytics should be enabled based on privacy settings
 */
export function shouldEnableAnalytics(config: AnalyticsConfig): boolean {
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
export function sanitizeEventName(eventName: string): string {
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
export function sanitizeParams(
  params?: AnalyticsEventParams
): Record<string, any> | undefined {
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
export function validateEvent(
  eventName: string,
  params?: AnalyticsEventParams
): boolean {
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

