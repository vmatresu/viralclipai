"use client";

import { usePathname } from "next/navigation";
import { useEffect } from "react";

import { analyticsEvents } from "@/lib/analytics";

/**
 * Hook to track page views automatically
 * Call this in page components or layout
 */
export function usePageView(pageName?: string) {
  const pathname = usePathname();

  useEffect(() => {
    const name = pageName ?? pathname ?? "unknown";
    const title = document.title ?? "";

    void analyticsEvents.pageViewed({
      pageName: name,
      pagePath: pathname ?? undefined,
      pageTitle: title || undefined,
    });
  }, [pathname, pageName]);
}
