"use client";

import { useCallback, useEffect, useState } from "react";
import {
  getSubscription,
  createCheckoutSession,
  previewPlanChange,
  changePlan,
  cancelSubscription,
  reactivateSubscription,
  getInvoices,
  getPaymentMethods,
  deletePaymentMethod,
  setDefaultPaymentMethod,
} from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import type {
  SubscriptionInfo,
  InvoicePreview,
  Invoice,
  PaymentMethod,
  PlanTier,
} from "@/types/billing";

export interface BillingState {
  subscription: SubscriptionInfo | null;
  invoices: Invoice[];
  paymentMethods: PaymentMethod[];
  loading: boolean;
  error: string | null;
  actionLoading: boolean;
  loadingPlan: PlanTier | undefined;
  paymentMethodLoadingId: string | undefined;
}

export interface BillingPreviewState {
  open: boolean;
  preview: InvoicePreview | null;
  targetPlan: PlanTier;
  targetPriceId: string;
}

export interface UseBillingReturn extends BillingState {
  preview: BillingPreviewState;
  isAuthenticated: boolean;
  authLoading: boolean;

  // Actions
  loadData: () => Promise<void>;
  retryLoad: () => void;
  clearError: () => void;
  handleSelectPlan: (priceId: string, planTier: PlanTier) => Promise<void>;
  handleConfirmChange: () => Promise<void>;
  handleCancel: () => Promise<void>;
  handleReactivate: () => Promise<void>;
  handleSetDefaultPaymentMethod: (paymentMethodId: string) => Promise<void>;
  handleDeletePaymentMethod: (paymentMethodId: string) => Promise<void>;
  closePreview: () => void;
}

/**
 * Hook for managing billing state and operations.
 * Provides loading states, error handling, and retry logic.
 */
export function useBilling(): UseBillingReturn {
  const { getIdToken, user, loading: authLoading } = useAuth();

  // Core state
  const [subscription, setSubscription] = useState<SubscriptionInfo | null>(null);
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [paymentMethods, setPaymentMethods] = useState<PaymentMethod[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Action states
  const [actionLoading, setActionLoading] = useState(false);
  const [loadingPlan, setLoadingPlan] = useState<PlanTier | undefined>();
  const [paymentMethodLoadingId, setPaymentMethodLoadingId] = useState<string | undefined>();

  // Preview modal state
  const [previewOpen, setPreviewOpen] = useState(false);
  const [preview, setPreview] = useState<InvoicePreview | null>(null);
  const [targetPlan, setTargetPlan] = useState<PlanTier>("free");
  const [targetPriceId, setTargetPriceId] = useState<string>("");

  // Auto-clear errors after 10 seconds
  useEffect(() => {
    if (error) {
      const timer = setTimeout(() => setError(null), 10000);
      return () => clearTimeout(timer);
    }
    return undefined;
  }, [error]);

  // Wrap errors with user-friendly messages
  const handleError = useCallback((err: unknown, context: string): string => {
    if (err instanceof Error) {
      // Network errors
      if (err.message.includes("Failed to fetch") || err.message.includes("NetworkError")) {
        return "Unable to connect. Please check your internet connection and try again.";
      }
      // Auth errors
      if (err.message.includes("401") || err.message.includes("Unauthorized")) {
        return "Your session has expired. Please sign in again.";
      }
      // Forbidden
      if (err.message.includes("403") || err.message.includes("Forbidden")) {
        return "You don't have permission to perform this action.";
      }
      return err.message;
    }
    return `${context} - Please try again.`;
  }, []);

  // Load billing data
  const loadData = useCallback(async () => {
    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to view billing information.");
        setLoading(false);
        return;
      }

      // Load all data in parallel, with graceful fallbacks
      const [subData, invoiceData, pmData] = await Promise.all([
        getSubscription(token),
        getInvoices(token, 10).catch((e) => {
          console.warn("Failed to load invoices:", e);
          return { invoices: [] };
        }),
        getPaymentMethods(token).catch((e) => {
          console.warn("Failed to load payment methods:", e);
          return { payment_methods: [] };
        }),
      ]);

      setSubscription(subData);
      setInvoices(invoiceData.invoices);
      setPaymentMethods(pmData.payment_methods);
      setError(null);
    } catch (err) {
      setError(handleError(err, "Failed to load billing data"));
    } finally {
      setLoading(false);
    }
  }, [getIdToken, handleError]);

  // Initial load
  useEffect(() => {
    if (!authLoading && user) {
      void loadData();
    } else if (!authLoading && !user) {
      setError("Please sign in to view billing information.");
      setLoading(false);
    }
  }, [authLoading, user, loadData]);

  // Retry load
  const retryLoad = useCallback(() => {
    setLoading(true);
    setError(null);
    void loadData();
  }, [loadData]);

  // Clear error manually
  const clearError = useCallback(() => setError(null), []);

  // Handle plan selection
  const handleSelectPlan = useCallback(async (priceId: string, planTier: PlanTier) => {
    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to continue.");
        return;
      }

      setLoadingPlan(planTier);
      setActionLoading(true);
      setError(null);

      // Downgrade to free - cancel subscription
      if (planTier === "free") {
        await cancelSubscription(token, false);
        await loadData();
        return;
      }

      // If user has no subscription, create a checkout session
      if (subscription?.plan === "free" || !subscription?.stripe_subscription_id) {
        const successUrl = `${window.location.origin}/billing/success?session_id={CHECKOUT_SESSION_ID}`;
        const cancelUrl = `${window.location.origin}/billing/cancel`;
        const result = await createCheckoutSession(token, priceId, successUrl, cancelUrl);
        window.location.href = result.checkout_url;
        return;
      }

      // Otherwise, preview the plan change
      const previewData = await previewPlanChange(token, priceId);
      setPreview(previewData);
      setTargetPlan(planTier);
      setTargetPriceId(priceId);
      setPreviewOpen(true);
    } catch (err) {
      setError(handleError(err, "Failed to process plan change"));
    } finally {
      setActionLoading(false);
      setLoadingPlan(undefined);
    }
  }, [getIdToken, subscription, loadData, handleError]);

  // Confirm plan change from preview modal
  const handleConfirmChange = useCallback(async () => {
    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to continue.");
        return;
      }

      setActionLoading(true);
      setError(null);
      await changePlan(token, targetPriceId, true);
      setPreviewOpen(false);
      await loadData();
    } catch (err) {
      setError(handleError(err, "Failed to change plan"));
    } finally {
      setActionLoading(false);
    }
  }, [getIdToken, targetPriceId, loadData, handleError]);

  // Cancel subscription
  const handleCancel = useCallback(async () => {
    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to continue.");
        return;
      }

      setActionLoading(true);
      setError(null);
      await cancelSubscription(token, false);
      await loadData();
    } catch (err) {
      setError(handleError(err, "Failed to cancel subscription"));
    } finally {
      setActionLoading(false);
    }
  }, [getIdToken, loadData, handleError]);

  // Reactivate subscription
  const handleReactivate = useCallback(async () => {
    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to continue.");
        return;
      }

      setActionLoading(true);
      setError(null);
      await reactivateSubscription(token);
      await loadData();
    } catch (err) {
      setError(handleError(err, "Failed to reactivate subscription"));
    } finally {
      setActionLoading(false);
    }
  }, [getIdToken, loadData, handleError]);

  // Set default payment method
  const handleSetDefaultPaymentMethod = useCallback(async (paymentMethodId: string) => {
    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to continue.");
        return;
      }

      setPaymentMethodLoadingId(paymentMethodId);
      setError(null);
      await setDefaultPaymentMethod(token, paymentMethodId);
      await loadData();
    } catch (err) {
      setError(handleError(err, "Failed to set default payment method"));
    } finally {
      setPaymentMethodLoadingId(undefined);
    }
  }, [getIdToken, loadData, handleError]);

  // Delete payment method
  const handleDeletePaymentMethod = useCallback(async (paymentMethodId: string) => {
    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to continue.");
        return;
      }

      setPaymentMethodLoadingId(paymentMethodId);
      setError(null);
      await deletePaymentMethod(token, paymentMethodId);
      await loadData();
    } catch (err) {
      setError(handleError(err, "Failed to delete payment method"));
    } finally {
      setPaymentMethodLoadingId(undefined);
    }
  }, [getIdToken, loadData, handleError]);

  // Close preview modal
  const closePreview = useCallback(() => {
    setPreviewOpen(false);
    setPreview(null);
  }, []);

  return {
    // State
    subscription,
    invoices,
    paymentMethods,
    loading,
    error,
    actionLoading,
    loadingPlan,
    paymentMethodLoadingId,
    isAuthenticated: !!user,
    authLoading,

    // Preview state
    preview: {
      open: previewOpen,
      preview,
      targetPlan,
      targetPriceId,
    },

    // Actions
    loadData,
    retryLoad,
    clearError,
    handleSelectPlan,
    handleConfirmChange,
    handleCancel,
    handleReactivate,
    handleSetDefaultPaymentMethod,
    handleDeletePaymentMethod,
    closePreview,
  };
}
