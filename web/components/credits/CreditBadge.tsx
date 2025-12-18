"use client";

import { cn } from "@/lib/utils";

export interface CreditBadgeProps {
  /** Credit amount to display */
  amount: number;
  /** Size variant */
  size?: "sm" | "md" | "lg";
  /** Visual variant */
  variant?: "default" | "highlight" | "warning" | "muted";
  /** Optional prefix like "+" or "-" */
  prefix?: string;
  /** Optional className for customization */
  className?: string;
}

/**
 * CreditBadge - A pill-style badge showing credit costs.
 * Used throughout the app to display credit amounts in a consistent way.
 */
export function CreditBadge({
  amount,
  size = "md",
  variant = "default",
  prefix = "",
  className,
}: CreditBadgeProps) {
  const sizeClasses = {
    sm: "text-xs px-1.5 py-0.5",
    md: "text-sm px-2 py-0.5",
    lg: "text-base px-3 py-1",
  };

  const variantClasses = {
    default: "bg-violet-500/10 text-violet-400 border-violet-500/20",
    highlight: "bg-violet-500/20 text-violet-300 border-violet-500/30 font-semibold",
    warning: "bg-amber-500/10 text-amber-400 border-amber-500/20",
    muted: "bg-slate-500/10 text-slate-400 border-slate-500/20",
  };

  return (
    <span
      className={cn(
        "inline-flex items-center gap-0.5 rounded-full border font-medium",
        sizeClasses[size],
        variantClasses[variant],
        className
      )}
    >
      {prefix && <span>{prefix}</span>}
      <span>{amount}</span>
      <span className="opacity-70">credits</span>
    </span>
  );
}
