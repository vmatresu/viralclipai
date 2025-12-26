"use client";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { formatPrice, type InvoicePreview, type PlanTier } from "@/types/billing";

interface InvoicePreviewModalProps {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  preview: InvoicePreview | null;
  targetPlan: PlanTier;
  isLoading?: boolean;
}

export function InvoicePreviewModal({
  open,
  onClose,
  onConfirm,
  preview,
  targetPlan,
  isLoading,
}: InvoicePreviewModalProps) {
  if (!preview) return null;

  const isCredit = preview.total_cents < 0;
  const isNoCharge = preview.total_cents === 0;

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Confirm Plan Change</DialogTitle>
          <DialogDescription>
            Review the charges before switching to {targetPlan.charAt(0).toUpperCase() + targetPlan.slice(1)}.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* Line items */}
          <div className="space-y-2">
            {preview.line_items.map((item, index) => (
              <div
                key={index}
                className="flex items-center justify-between text-sm"
              >
                <span
                  className={
                    item.proration ? "text-muted-foreground" : undefined
                  }
                >
                  {item.description}
                  {item.proration && (
                    <span className="ml-1 text-xs">(prorated)</span>
                  )}
                </span>
                <span
                  className={
                    item.amount_cents < 0 ? "text-green-600" : undefined
                  }
                >
                  {formatPrice(item.amount_cents, item.currency)}
                </span>
              </div>
            ))}
          </div>

          {/* Divider */}
          <div className="border-t" />

          {/* Totals */}
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">Subtotal</span>
              <span>{formatPrice(preview.subtotal_cents, preview.currency)}</span>
            </div>
            <div className="flex items-center justify-between font-medium">
              <span>
                {isCredit ? "Credit to Account" : isNoCharge ? "Total" : "Amount Due"}
              </span>
              <span className={isCredit ? "text-green-600" : undefined}>
                {formatPrice(preview.total_cents, preview.currency)}
              </span>
            </div>
          </div>

          {/* Summary */}
          <div className="rounded-lg bg-muted p-3 text-sm">
            {preview.summary}
          </div>
        </div>

        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={onClose} disabled={isLoading}>
            Cancel
          </Button>
          <Button onClick={onConfirm} disabled={isLoading}>
            {isLoading
              ? "Processing..."
              : isNoCharge
                ? "Confirm Change"
                : isCredit
                  ? "Confirm & Apply Credit"
                  : "Confirm & Pay"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
