/**
 * Environment variable validation and configuration
 * Centralized environment variable management with validation
 */

import { requireEnv, isValidFirebaseConfig } from "./validation";

/**
 * Validated environment variables
 * All environment variables should be accessed through this object
 */
export const env = {
  // API Configuration
  apiBaseUrl: process.env.NEXT_PUBLIC_API_BASE_URL ?? "",

  // Firebase Configuration
  firebase: {
    apiKey: process.env.NEXT_PUBLIC_FIREBASE_API_KEY ?? "",
    authDomain: process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN ?? "",
    projectId: process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID ?? "",
    storageBucket: process.env.NEXT_PUBLIC_FIREBASE_STORAGE_BUCKET ?? "",
    messagingSenderId: process.env.NEXT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID ?? "",
    appId: process.env.NEXT_PUBLIC_FIREBASE_APP_ID ?? "",
    measurementId: process.env.NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID ?? "",
  },

  // Environment
  nodeEnv: process.env.NODE_ENV ?? "development",
  isDevelopment: process.env.NODE_ENV === "development",
  isProduction: process.env.NODE_ENV === "production",
} as const;

/**
 * Validates that all required environment variables are present
 * Call this function at application startup
 */
export function validateEnvironment(): void {
  if (typeof window === "undefined") {
    // Server-side validation
    // Add any server-only required env vars here
    return;
  }

  // Client-side validation
  // Firebase config is required for auth to work
  if (
    !isValidFirebaseConfig({
      apiKey: env.firebase.apiKey,
      authDomain: env.firebase.authDomain,
      projectId: env.firebase.projectId,
    })
  ) {
    console.warn(
      "[Security] Firebase configuration is incomplete. Auth features may not work correctly."
    );
  }
}

/**
 * Gets a validated environment variable
 * Throws an error if the variable is missing or empty
 */
export function getRequiredEnv(name: string): string {
  const value = process.env[name];
  return requireEnv(value, name);
}

// Validate environment on module load (client-side only)
if (typeof window !== "undefined") {
  validateEnvironment();
}
