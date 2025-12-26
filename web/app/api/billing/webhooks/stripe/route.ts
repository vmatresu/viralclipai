import { NextResponse } from "next/server";
import Stripe from "stripe";
import { stripe, STRIPE_WEBHOOK_SECRET } from "@/lib/server/stripe";
import {
  handleCheckoutCompleted,
  handleSubscriptionUpdated,
  handleSubscriptionDeleted,
  handleInvoicePaid,
  handleInvoicePaymentFailed,
} from "@/lib/server/billing";

/**
 * POST /api/billing/webhooks/stripe
 * Handle Stripe webhook events
 *
 * This endpoint is unauthenticated but requires valid Stripe signature.
 */
export async function POST(request: Request) {
  // Get raw body for signature verification
  const body = await request.text();

  // Get Stripe signature header
  const signature = request.headers.get("stripe-signature");

  if (!signature) {
    console.warn("Missing Stripe-Signature header");
    return NextResponse.json(
      { error: "Missing Stripe-Signature header" },
      { status: 400 }
    );
  }

  if (!STRIPE_WEBHOOK_SECRET) {
    console.error("STRIPE_WEBHOOK_SECRET is not configured");
    return NextResponse.json(
      { error: "Webhook not configured" },
      { status: 500 }
    );
  }

  // Verify signature using official Stripe SDK
  let event: Stripe.Event;
  try {
    event = stripe.webhooks.constructEvent(body, signature, STRIPE_WEBHOOK_SECRET);
  } catch (err) {
    const message = err instanceof Error ? err.message : "Unknown error";
    console.warn(`Webhook signature verification failed: ${message}`);
    return NextResponse.json(
      { error: "Invalid signature" },
      { status: 400 }
    );
  }

  console.log(`Processing Stripe webhook: ${event.type} (${event.id})`);

  // Handle the event
  try {
    switch (event.type) {
      case "checkout.session.completed": {
        const session = event.data.object as Stripe.Checkout.Session;
        await handleCheckoutCompleted(session);
        break;
      }

      case "customer.subscription.created":
      case "customer.subscription.updated": {
        const subscription = event.data.object as Stripe.Subscription;
        await handleSubscriptionUpdated(subscription);
        break;
      }

      case "customer.subscription.deleted": {
        const subscription = event.data.object as Stripe.Subscription;
        await handleSubscriptionDeleted(subscription);
        break;
      }

      case "invoice.paid": {
        const invoice = event.data.object as Stripe.Invoice;
        await handleInvoicePaid(invoice);
        break;
      }

      case "invoice.payment_failed": {
        const invoice = event.data.object as Stripe.Invoice;
        await handleInvoicePaymentFailed(invoice);
        break;
      }

      // Additional events for comprehensive billing
      case "customer.subscription.paused": {
        const subscription = event.data.object as Stripe.Subscription;
        console.log(`Subscription ${subscription.id} paused`);
        await handleSubscriptionUpdated(subscription);
        break;
      }

      case "customer.subscription.resumed": {
        const subscription = event.data.object as Stripe.Subscription;
        console.log(`Subscription ${subscription.id} resumed`);
        await handleSubscriptionUpdated(subscription);
        break;
      }

      case "invoice.payment_action_required": {
        const invoice = event.data.object as Stripe.Invoice;
        console.log(`Payment action required for invoice ${invoice.id}`);
        // Could send notification to user about 3D Secure
        break;
      }

      case "payment_method.attached": {
        const paymentMethod = event.data.object as Stripe.PaymentMethod;
        console.log(`Payment method ${paymentMethod.id} attached`);
        break;
      }

      case "payment_method.detached": {
        const paymentMethod = event.data.object as Stripe.PaymentMethod;
        console.log(`Payment method ${paymentMethod.id} detached`);
        break;
      }

      default:
        console.log(`Unhandled event type: ${event.type}`);
    }

    return NextResponse.json({ received: true });
  } catch (error) {
    console.error(`Error processing webhook ${event.type}:`, error);
    // Return 200 to prevent Stripe from retrying (we logged the error)
    return NextResponse.json({ received: true, error: "Processing failed" });
  }
}

// Disable body parser for webhook - we need raw body for signature verification
export const config = {
  api: {
    bodyParser: false,
  },
};
