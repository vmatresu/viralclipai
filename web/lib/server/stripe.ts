import Stripe from "stripe";

if (!process.env.STRIPE_SECRET_KEY) {
  throw new Error("STRIPE_SECRET_KEY is not set");
}

/**
 * Server-side Stripe client using the official SDK.
 *
 * Uses the latest Stripe API version for full compatibility.
 */
export const stripe = new Stripe(process.env.STRIPE_SECRET_KEY, {
  apiVersion: "2025-12-15.clover", // Latest API version
  typescript: true,
});

/**
 * Stripe price configuration from environment variables.
 * Empty strings are filtered out in getAllPriceIds().
 */
export const STRIPE_PRICES = {
  pro: {
    monthly: process.env.STRIPE_PRICE_PRO_MONTHLY ?? "",
    annual: process.env.STRIPE_PRICE_PRO_ANNUAL ?? "",
  },
  studio: {
    monthly: process.env.STRIPE_PRICE_STUDIO_MONTHLY ?? "",
    annual: process.env.STRIPE_PRICE_STUDIO_ANNUAL ?? "",
  },
} as const;

/**
 * Get all valid price IDs.
 */
export function getAllPriceIds(): string[] {
  return [
    STRIPE_PRICES.pro.monthly,
    STRIPE_PRICES.pro.annual,
    STRIPE_PRICES.studio.monthly,
    STRIPE_PRICES.studio.annual,
  ].filter(Boolean);
}

/**
 * Check if a price ID is valid.
 */
export function isValidPriceId(priceId: string): boolean {
  return getAllPriceIds().includes(priceId);
}

/**
 * Get plan tier from price ID.
 */
export function getPlanTierFromPriceId(priceId: string): "pro" | "studio" | null {
  if (priceId === STRIPE_PRICES.pro.monthly || priceId === STRIPE_PRICES.pro.annual) {
    return "pro";
  }
  if (priceId === STRIPE_PRICES.studio.monthly || priceId === STRIPE_PRICES.studio.annual) {
    return "studio";
  }
  return null;
}

/**
 * Get billing interval from price ID.
 */
export function getBillingInterval(priceId: string): "monthly" | "annual" | null {
  if (priceId === STRIPE_PRICES.pro.monthly || priceId === STRIPE_PRICES.studio.monthly) {
    return "monthly";
  }
  if (priceId === STRIPE_PRICES.pro.annual || priceId === STRIPE_PRICES.studio.annual) {
    return "annual";
  }
  return null;
}

/**
 * Webhook secret for signature verification.
 * May be undefined if not configured (checked at runtime in webhook handler).
 */
export const STRIPE_WEBHOOK_SECRET = process.env.STRIPE_WEBHOOK_SECRET;
