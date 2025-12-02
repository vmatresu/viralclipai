/**
 * Analytics Configuration & Constants
 * 
 * Centralized configuration and constants for Firebase Analytics.
 */

import { AnalyticsConfig } from "./types";

/**
 * Default analytics configuration
 */
export const DEFAULT_CONFIG: AnalyticsConfig = {
  enabled: true,
  debug: process.env.NODE_ENV === "development",
  respectDoNotTrack: true,
  maxEventQueueSize: 50,
  batchIntervalMs: 1000,
};

/**
 * Firebase Analytics limits
 */
export const MAX_EVENT_NAME_LENGTH = 40;
export const MAX_PARAM_NAME_LENGTH = 40;
export const MAX_PARAM_VALUE_LENGTH = 100;
export const MAX_PARAMS_PER_EVENT = 25;

