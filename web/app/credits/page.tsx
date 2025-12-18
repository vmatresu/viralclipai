"use client";

import { Suspense } from "react";

import { PageWrapper } from "@/components/landing";

import CreditHistoryList from "./CreditHistoryList";

function CreditsPageContent() {
  return (
    <PageWrapper>
      <CreditHistoryList />
    </PageWrapper>
  );
}

export default function CreditsPage() {
  return (
    <Suspense fallback={<div className="text-muted-foreground">Loading...</div>}>
      <CreditsPageContent />
    </Suspense>
  );
}
