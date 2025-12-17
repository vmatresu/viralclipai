"use client";

import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useEffect } from "react";

import { PageWrapper } from "@/components/landing";

import HistoryList from "./HistoryList";

/**
 * History Page Content
 *
 * Handles legacy ?id= query params by redirecting to /history/[id]
 * Otherwise renders the HistoryList component
 */
function HistoryPageContent() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const videoId = searchParams.get("id");

  useEffect(() => {
    // Redirect legacy ?id= URLs to the new /history/[id] route
    if (videoId) {
      router.replace(`/history/${encodeURIComponent(videoId)}`);
    }
  }, [videoId, router]);

  // Show loading while redirecting
  if (videoId) {
    return (
      <PageWrapper>
        <div className="flex flex-col items-center justify-center py-24 space-y-4">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
          <p className="text-muted-foreground">Redirecting...</p>
        </div>
      </PageWrapper>
    );
  }

  return (
    <PageWrapper>
      <HistoryList />
    </PageWrapper>
  );
}

export default function HistoryPage() {
  return (
    <Suspense fallback={<div className="text-muted-foreground">Loading...</div>}>
      <HistoryPageContent />
    </Suspense>
  );
}
