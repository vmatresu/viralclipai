/**
 * Billing module.
 *
 * Provides Stripe billing functionality including:
 * - Customer management
 * - Subscription operations (checkout, plan changes, cancellation)
 * - Payment method management
 * - Invoice history
 * - Webhook event handling
 *
 * Architecture:
 * - types.ts: Type definitions
 * - repository.ts: Firestore data access layer
 * - customers.ts: Stripe customer management
 * - subscriptions.ts: Subscription operations
 * - payment-methods.ts: Payment method CRUD
 * - invoices.ts: Invoice operations
 * - webhooks.ts: Webhook event handlers
 */

// Types
export type {
  UserBillingData,
  SubscriptionInfo,
  PlanTier,
  SubscriptionStatus,
  BillingInterval,
  InvoicePreview,
  InvoiceLineItem,
  Invoice,
  PaymentMethod,
  CheckoutResult,
} from "./types";

// Customer management
export { getOrCreateCustomer } from "./customers";

// Subscription operations
export {
  getSubscriptionInfo,
  createCheckoutSession,
  previewPlanChange,
  changePlan,
  cancelSubscription,
  reactivateSubscription,
  syncSubscriptionToFirestore,
} from "./subscriptions";

// Payment methods
export {
  getPaymentMethods,
  addPaymentMethod,
  deletePaymentMethod,
  setDefaultPaymentMethod,
} from "./payment-methods";

// Invoices
export { getInvoices } from "./invoices";

// Webhook handlers
export {
  handleCheckoutCompleted,
  handleSubscriptionUpdated,
  handleSubscriptionDeleted,
  handleInvoicePaid,
  handleInvoicePaymentFailed,
} from "./webhooks";
