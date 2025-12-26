"use client";

import { X, RefreshCw } from "lucide-react";
import { PageWrapper } from "@/components/landing";
import {
  PlanSelector,
  CurrentPlanCard,
  InvoicePreviewModal,
  InvoiceHistory,
  PaymentMethodList,
} from "@/components/billing";
import { Button } from "@/components/ui/button";
import { useBilling } from "@/lib/hooks/useBilling";

export default function BillingPage() {
  const billing = useBilling();

  // Show loading state
  if (billing.loading || billing.authLoading) {
    return (
      <PageWrapper>
        <div className="flex items-center justify-center py-12">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
        </div>
      </PageWrapper>
    );
  }

  // Show error if not authenticated
  if (!billing.isAuthenticated || !billing.subscription) {
    return (
      <PageWrapper>
        <div className="glass rounded-2xl p-6 text-center space-y-4">
          <p className="text-muted-foreground">
            {billing.error ?? "Please sign in to view billing information."}
          </p>
          {billing.error && (
            <Button onClick={billing.retryLoad} variant="outline" size="sm">
              <RefreshCw className="mr-2 h-4 w-4" />
              Try Again
            </Button>
          )}
        </div>
      </PageWrapper>
    );
  }

  return (
    <PageWrapper>
      <div className="space-y-8">
        {/* Page header */}
        <div>
          <h1 className="text-2xl font-bold text-foreground">Billing</h1>
          <p className="text-muted-foreground">
            Manage your subscription and billing information.
          </p>
        </div>

        {/* Error message with dismiss */}
        {billing.error && (
          <div className="flex items-center justify-between rounded-lg bg-destructive/10 p-4 text-sm text-destructive">
            <span>{billing.error}</span>
            <button
              onClick={billing.clearError}
              className="ml-4 rounded-full p-1 hover:bg-destructive/20"
              aria-label="Dismiss error"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        )}

        {/* Current plan card */}
        <CurrentPlanCard
          subscription={billing.subscription}
          onCancelClick={() => {
            if (confirm("Are you sure you want to cancel your subscription? You will still have access until the end of your billing period.")) {
              void billing.handleCancel();
            }
          }}
          onReactivateClick={billing.handleReactivate}
          isLoading={billing.actionLoading}
        />

        {/* Plan selector */}
        <div className="space-y-4">
          <h2 className="text-xl font-semibold text-foreground">Change Plan</h2>
          <PlanSelector
            currentSubscription={billing.subscription}
            onSelectPlan={billing.handleSelectPlan}
            isLoading={billing.actionLoading}
            loadingPlan={billing.loadingPlan}
          />
        </div>

        {/* Payment methods */}
        <PaymentMethodList
          paymentMethods={billing.paymentMethods}
          onSetDefault={billing.handleSetDefaultPaymentMethod}
          onDelete={(id) => {
            if (confirm("Are you sure you want to remove this payment method?")) {
              void billing.handleDeletePaymentMethod(id);
            }
          }}
          isLoading={!!billing.paymentMethodLoadingId}
          loadingId={billing.paymentMethodLoadingId}
        />

        {/* Invoice history */}
        <InvoiceHistory invoices={billing.invoices} />
      </div>

      {/* Invoice preview modal */}
      <InvoicePreviewModal
        open={billing.preview.open}
        onClose={billing.closePreview}
        onConfirm={billing.handleConfirmChange}
        preview={billing.preview.preview}
        targetPlan={billing.preview.targetPlan}
        isLoading={billing.actionLoading}
      />
    </PageWrapper>
  );
}
