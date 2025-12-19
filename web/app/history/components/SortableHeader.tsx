"use client";

import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";

import { cn } from "@/lib/utils";

import type { SortDirection, SortField } from "../hooks";

interface SortableHeaderProps {
  field: SortField;
  currentField: SortField;
  direction: SortDirection;
  onSort: (field: SortField) => void;
  children: React.ReactNode;
}

/**
 * Clickable table header with sort indicators.
 */
export function SortableHeader({
  field,
  currentField,
  direction,
  onSort,
  children,
}: SortableHeaderProps) {
  const isActive = currentField === field;

  return (
    <button
      onClick={() => onSort(field)}
      className={cn(
        "flex items-center gap-1 hover:text-foreground transition-colors",
        isActive ? "text-foreground" : "text-muted-foreground"
      )}
    >
      {children}
      {isActive && direction === "asc" && <ArrowUp className="h-3.5 w-3.5" />}
      {isActive && direction === "desc" && <ArrowDown className="h-3.5 w-3.5" />}
      {!isActive && <ArrowUpDown className="h-3.5 w-3.5 opacity-50" />}
    </button>
  );
}
