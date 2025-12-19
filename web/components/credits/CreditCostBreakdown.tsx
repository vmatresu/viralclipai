"use client";

import { cn } from "@/lib/utils";

export interface CreditLineItem {
  /** Description of the line item */
  label: string;
  /** Quantity (number of items) */
  qty: number;
  /** Cost per unit in credits */
  unitCost: number;
  /** Total cost for this line (qty * unitCost) */
  totalCost: number;
  /** Optional note/description */
  note?: string;
}

export interface CreditCostBreakdownProps {
  /** Line items to display */
  lineItems: CreditLineItem[];
  /** Total credits for all items */
  total: number;
  /** Remaining credits after this operation (optional) */
  remaining?: number;
  /** Whether user exceeds quota */
  exceedsQuota?: boolean;
  /** Optional className */
  className?: string;
}

/**
 * CreditCostBreakdown - Shows a detailed breakdown of credit costs.
 * Displays line items with quantities and costs, plus totals.
 */
export function CreditCostBreakdown({
  lineItems,
  total,
  remaining,
  exceedsQuota,
  className,
}: CreditCostBreakdownProps) {
  const visibleItems = lineItems.filter((item) => item.qty > 0);

  if (visibleItems.length === 0) {
    return (
      <div className={cn("text-sm text-slate-500", className)}>
        Select items to see cost breakdown
      </div>
    );
  }

  return (
    <div className={cn("space-y-3", className)}>
      {/* Line items */}
      <div className="space-y-2">
        {visibleItems.map((item) => (
          <div
            key={`${item.label}-${item.unitCost}-${item.qty}-${item.totalCost}`}
            className="flex items-center justify-between text-sm"
          >
            <div className="flex items-center gap-2">
              <span className="text-slate-300">{item.label}</span>
              {item.qty > 1 && (
                <span className="text-slate-500">
                  ({item.qty} × {item.unitCost})
                </span>
              )}
              {item.note && <span className="text-xs text-slate-500">{item.note}</span>}
            </div>
            <span className="font-medium text-slate-200">{item.totalCost} credits</span>
          </div>
        ))}
      </div>

      {/* Divider */}
      <div className="border-t border-slate-700/50" />

      {/* Total */}
      <div className="flex items-center justify-between">
        <span className="font-semibold text-white">Total</span>
        <span
          className={cn(
            "text-lg font-bold",
            exceedsQuota ? "text-red-400" : "text-violet-400"
          )}
        >
          {total} credits
        </span>
      </div>

      {/* Remaining */}
      {remaining !== undefined && (
        <div className="flex items-center justify-between text-sm">
          <span className="text-slate-400">Remaining after</span>
          <span
            className={cn(
              "font-medium",
              remaining < 0 ? "text-red-400" : "text-slate-300"
            )}
          >
            {remaining} credits
          </span>
        </div>
      )}

      {/* Warning if exceeds quota */}
      {exceedsQuota && (
        <div className="p-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-xs">
          This exceeds your monthly credit limit. Please upgrade your plan or select
          fewer items.
        </div>
      )}
    </div>
  );
}
