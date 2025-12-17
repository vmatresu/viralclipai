"use client";

import { cn } from "@/lib/utils";

interface LandingSectionProps {
  children: React.ReactNode;
  id?: string;
  className?: string;
  containerClassName?: string;
}

export function LandingSection({
  children,
  id,
  className,
  containerClassName,
}: LandingSectionProps) {
  return (
    <section id={id} className={cn("section-padding relative", className)}>
      <div className={cn("landing-container", containerClassName)}>{children}</div>
    </section>
  );
}

interface SectionHeaderProps {
  title: React.ReactNode;
  description?: React.ReactNode;
  className?: string;
}

export function SectionHeader({ title, description, className }: SectionHeaderProps) {
  return (
    <div className={cn("text-center max-w-3xl mx-auto mb-16", className)}>
      <h2 className="text-3xl md:text-4xl font-bold leading-tight mb-6">{title}</h2>
      {description && (
        <p className="text-lg text-muted-foreground leading-relaxed">{description}</p>
      )}
    </div>
  );
}
