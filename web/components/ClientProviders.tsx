"use client";

/**
 * Client-side providers wrapper
 *
 * Wraps client-only providers that need to be used in the server-rendered layout.
 */

import { type ReactNode } from "react";

import { ProcessingProvider } from "@/lib/processing-context";

export function ClientProviders({ children }: { children: ReactNode }) {
  return <ProcessingProvider>{children}</ProcessingProvider>;
}
