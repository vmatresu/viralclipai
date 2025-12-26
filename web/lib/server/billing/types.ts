/**
 * Billing module type definitions.
 *
 * Shared types for Stripe billing operations.
 */

/** User billing data stored in Firestore */
export interface UserBillingData {
  stripe_customer_id?: string;
  stripe_subscription_id?: string;
  subscription_status?: string;
  current_period_end?: number;
  cancel_at_period_end?: boolean;
  plan?: string;
}

/** Subscription information returned to clients */
export interface SubscriptionInfo {
  plan: PlanTier;
  status: SubscriptionStatus;
  interval?: BillingInterval;
  current_period_start?: string;
  current_period_end?: string;
  cancel_at_period_end: boolean;
  stripe_subscription_id?: string;
  stripe_customer_id?: string;
}

/** Plan tiers */
export type PlanTier = "free" | "pro" | "studio";

/** Subscription status */
export type SubscriptionStatus = "none" | "active" | "past_due" | "canceled" | "trialing";

/** Billing interval */
export type BillingInterval = "monthly" | "annual";

/** Invoice preview for plan changes */
export interface InvoicePreview {
  line_items: InvoiceLineItem[];
  subtotal_cents: number;
  total_cents: number;
  currency: string;
  summary: string;
}

/** Invoice line item */
export interface InvoiceLineItem {
  description: string;
  amount_cents: number;
  currency: string;
  proration: boolean;
}

/** Invoice record */
export interface Invoice {
  id: string;
  number?: string;
  amount_cents: number;
  currency: string;
  status: string;
  created_at: string;
  hosted_invoice_url?: string;
  invoice_pdf_url?: string;
}

/** Payment method (card) */
export interface PaymentMethod {
  id: string;
  brand: string;
  last4: string;
  exp_month: number;
  exp_year: number;
  is_default: boolean;
}

/** Checkout session result */
export interface CheckoutResult {
  checkout_url: string;
  session_id: string;
}
