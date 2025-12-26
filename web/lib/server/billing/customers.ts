/**
 * Stripe customer management.
 *
 * Handles creation and retrieval of Stripe customers.
 */

import { stripe } from "../stripe";
import { getUserBillingData, updateUserBillingData } from "./repository";

/**
 * Get existing Stripe customer or create a new one.
 *
 * Links the Stripe customer to the Firebase user via metadata.
 */
export async function getOrCreateCustomer(
  uid: string,
  email: string
): Promise<string> {
  const billingData = await getUserBillingData(uid);

  // Check if customer already exists in our records
  if (billingData.stripe_customer_id) {
    try {
      // Verify customer still exists in Stripe
      await stripe.customers.retrieve(billingData.stripe_customer_id);
      return billingData.stripe_customer_id;
    } catch {
      // Customer was deleted from Stripe, create a new one
    }
  }

  // Create new customer in Stripe
  const customer = await stripe.customers.create({
    email,
    metadata: {
      firebase_uid: uid,
    },
  });

  // Save customer ID to Firestore
  await updateUserBillingData(uid, {
    stripe_customer_id: customer.id,
  });

  return customer.id;
}
