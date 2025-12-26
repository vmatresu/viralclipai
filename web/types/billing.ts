/**
 * Billing and subscription types for Stripe integration.
 */

// ============================================================================
// Subscription Types
// ============================================================================

export type PlanTier = "free" | "pro" | "studio";

export type SubscriptionStatus =
  | "active"
  | "canceled"
  | "past_due"
  | "trialing"
  | "incomplete"
  | "none";

export type BillingInterval = "monthly" | "annual";

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

// ============================================================================
// Checkout Types
// ============================================================================

export interface CreateCheckoutRequest {
  price_id: string;
  success_url: string;
  cancel_url: string;
}

export interface CreateCheckoutResponse {
  checkout_url: string;
  session_id: string;
}

// ============================================================================
// Plan Change Types
// ============================================================================

export interface PreviewChangeRequest {
  new_price_id: string;
}

export interface InvoiceLineItem {
  description: string;
  amount_cents: number;
  currency: string;
  proration: boolean;
}

export interface InvoicePreview {
  line_items: InvoiceLineItem[];
  subtotal_cents: number;
  total_cents: number;
  currency: string;
  billing_date?: string;
  summary: string;
}

export interface ChangePlanRequest {
  new_price_id: string;
  prorate: boolean;
}

// ============================================================================
// Cancellation Types
// ============================================================================

export interface CancelSubscriptionRequest {
  immediately: boolean;
}

// ============================================================================
// Invoice Types
// ============================================================================

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

// ============================================================================
// Payment Method Types
// ============================================================================

export interface PaymentMethod {
  id: string;
  brand: string;
  last4: string;
  exp_month: number;
  exp_year: number;
  is_default: boolean;
}

export interface AddPaymentMethodRequest {
  payment_method_id: string;
  set_as_default: boolean;
}

export interface SetDefaultPaymentMethodRequest {
  payment_method_id: string;
}

// ============================================================================
// Price Configuration
// ============================================================================

export interface PriceConfig {
  pro_monthly: string;
  pro_annual: string;
  studio_monthly: string;
  studio_annual: string;
}

/**
 * Price IDs for each plan/interval combination.
 * These should match your Stripe dashboard.
 */
export const PRICE_IDS: PriceConfig = {
  pro_monthly: process.env.NEXT_PUBLIC_STRIPE_PRICE_PRO_MONTHLY ?? "",
  pro_annual: process.env.NEXT_PUBLIC_STRIPE_PRICE_PRO_ANNUAL ?? "",
  studio_monthly: process.env.NEXT_PUBLIC_STRIPE_PRICE_STUDIO_MONTHLY ?? "",
  studio_annual: process.env.NEXT_PUBLIC_STRIPE_PRICE_STUDIO_ANNUAL ?? "",
};

/**
 * Plan display information
 */
export interface PlanInfo {
  tier: PlanTier;
  name: string;
  monthlyPrice: number;
  annualPrice: number;
  annualSavings: number;
  credits: number;
  features: string[];
}

export const PLANS: PlanInfo[] = [
  {
    tier: "free",
    name: "Free",
    monthlyPrice: 0,
    annualPrice: 0,
    annualSavings: 0,
    credits: 30,
    features: [
      "30 credits/month",
      "Basic clip export",
      "720p resolution",
      "Standard processing",
    ],
  },
  {
    tier: "pro",
    name: "Pro",
    monthlyPrice: 29,
    annualPrice: 290,
    annualSavings: 58, // 2 months free
    credits: 150,
    features: [
      "150 credits/month",
      "All export styles",
      "1080p resolution",
      "Priority processing",
      "Email support",
    ],
  },
  {
    tier: "studio",
    name: "Studio",
    monthlyPrice: 99,
    annualPrice: 990,
    annualSavings: 198, // 2 months free
    credits: 500,
    features: [
      "500 credits/month",
      "All export styles",
      "4K resolution",
      "Fastest processing",
      "Priority support",
      "Custom branding",
    ],
  },
];

/**
 * Get price ID for a plan and interval
 */
export function getPriceId(
  plan: PlanTier,
  interval: BillingInterval
): string | null {
  if (plan === "free") return null;

  const key = `${plan}_${interval}` as keyof PriceConfig;
  return PRICE_IDS[key] || null;
}

/**
 * Get plan info from price ID
 */
export function getPlanFromPriceId(priceId: string): {
  plan: PlanTier;
  interval: BillingInterval;
} | null {
  if (priceId === PRICE_IDS.pro_monthly) {
    return { plan: "pro", interval: "monthly" };
  }
  if (priceId === PRICE_IDS.pro_annual) {
    return { plan: "pro", interval: "annual" };
  }
  if (priceId === PRICE_IDS.studio_monthly) {
    return { plan: "studio", interval: "monthly" };
  }
  if (priceId === PRICE_IDS.studio_annual) {
    return { plan: "studio", interval: "annual" };
  }
  return null;
}

/**
 * Format cents to display price
 */
export function formatPrice(cents: number, currency = "usd"): string {
  const amount = Math.abs(cents) / 100;
  const formatted = new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: currency.toUpperCase(),
  }).format(amount);

  return cents < 0 ? `-${formatted}` : formatted;
}

/**
 * Format date for display
 */
export function formatDate(dateString: string): string {
  return new Date(dateString).toLocaleDateString("en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}
