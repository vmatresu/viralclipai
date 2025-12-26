/**
 * Payment method management.
 *
 * Handles listing, adding, and removing payment methods.
 */

import { stripe } from "../stripe";
import { getUserBillingData } from "./repository";
import { getOrCreateCustomer } from "./customers";
import type { PaymentMethod } from "./types";

/**
 * List payment methods for a user.
 */
export async function getPaymentMethods(uid: string): Promise<PaymentMethod[]> {
  const billingData = await getUserBillingData(uid);

  if (!billingData.stripe_customer_id) {
    return [];
  }

  // Get customer to find default payment method
  const customer = await stripe.customers.retrieve(billingData.stripe_customer_id);
  const defaultPmId = getDefaultPaymentMethodId(customer);

  const paymentMethods = await stripe.paymentMethods.list({
    customer: billingData.stripe_customer_id,
    type: "card",
  });

  return paymentMethods.data
    .filter((pm) => pm.card)
    .map((pm) => ({
      id: pm.id,
      brand: pm.card!.brand,
      last4: pm.card!.last4,
      exp_month: pm.card!.exp_month,
      exp_year: pm.card!.exp_year,
      is_default: pm.id === defaultPmId,
    }));
}

/**
 * Extract default payment method ID from customer object.
 */
function getDefaultPaymentMethodId(
  customer: Awaited<ReturnType<typeof stripe.customers.retrieve>>
): string | null {
  if (typeof customer === "string" || customer.deleted) {
    return null;
  }

  const defaultPm = customer.invoice_settings?.default_payment_method;
  if (!defaultPm) {
    return null;
  }

  return typeof defaultPm === "string" ? defaultPm : defaultPm.id;
}

/**
 * Add a payment method to a user's account.
 *
 * @param paymentMethodId - The Stripe payment method ID (from Stripe.js)
 * @param setAsDefault - Whether to set as the default payment method
 */
export async function addPaymentMethod(
  uid: string,
  email: string,
  paymentMethodId: string,
  setAsDefault: boolean = false
): Promise<PaymentMethod> {
  const customerId = await getOrCreateCustomer(uid, email);

  // Attach payment method to customer
  const pm = await stripe.paymentMethods.attach(paymentMethodId, {
    customer: customerId,
  });

  // Set as default if requested
  if (setAsDefault) {
    await stripe.customers.update(customerId, {
      invoice_settings: {
        default_payment_method: paymentMethodId,
      },
    });
  }

  if (!pm.card) {
    throw new Error("Payment method has no card details");
  }

  return {
    id: pm.id,
    brand: pm.card.brand,
    last4: pm.card.last4,
    exp_month: pm.card.exp_month,
    exp_year: pm.card.exp_year,
    is_default: setAsDefault,
  };
}

/**
 * Delete a payment method.
 *
 * Verifies ownership before deletion for security.
 */
export async function deletePaymentMethod(
  uid: string,
  paymentMethodId: string
): Promise<void> {
  // Verify ownership first - SECURITY: prevents users from deleting other users' cards
  const methods = await getPaymentMethods(uid);
  const owns = methods.some((m) => m.id === paymentMethodId);

  if (!owns) {
    throw new Error("Payment method does not belong to this user");
  }

  await stripe.paymentMethods.detach(paymentMethodId);
}

/**
 * Set a payment method as the default.
 *
 * Verifies ownership before updating for security.
 */
export async function setDefaultPaymentMethod(
  uid: string,
  paymentMethodId: string
): Promise<void> {
  const billingData = await getUserBillingData(uid);

  if (!billingData.stripe_customer_id) {
    throw new Error("No Stripe customer found");
  }

  // Verify ownership first - SECURITY: prevents users from manipulating other accounts
  const methods = await getPaymentMethods(uid);
  const owns = methods.some((m) => m.id === paymentMethodId);

  if (!owns) {
    throw new Error("Payment method does not belong to this user");
  }

  await stripe.customers.update(billingData.stripe_customer_id, {
    invoice_settings: {
      default_payment_method: paymentMethodId,
    },
  });
}
