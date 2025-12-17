"use client";

import { cn } from "@/lib/utils";

interface GlassCardProps {
  children: React.ReactNode;
  className?: string;
  hover?: boolean;
}

export function GlassCard({ children, className, hover = true }: GlassCardProps) {
  return (
    <div
      className={cn(
        "glass-card rounded-2xl p-8 transition-all duration-300",
        hover && "hover:-translate-y-2",
        className
      )}
    >
      {children}
    </div>
  );
}
