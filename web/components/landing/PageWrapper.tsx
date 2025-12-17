"use client";

import { cn } from "@/lib/utils";

interface PageWrapperProps {
  children: React.ReactNode;
  className?: string;
}

/**
 * Wrapper for non-landing pages (history, settings, etc.)
 * Provides consistent padding and max-width
 */
export function PageWrapper({ children, className }: PageWrapperProps) {
  return (
    <div className={cn("landing-container pt-32 pb-12 space-y-8", className)}>
      {children}
    </div>
  );
}
