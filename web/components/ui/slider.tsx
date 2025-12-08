"use client";

import * as SliderPrimitive from "@radix-ui/react-slider";
import * as React from "react";

import { cn } from "@/lib/utils";

const Slider = React.forwardRef<
  React.ElementRef<typeof SliderPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SliderPrimitive.Root>
>(({ className, ...props }, ref) => (
  <SliderPrimitive.Root
    ref={ref}
    className={cn(
      "relative flex w-full touch-none select-none items-center",
      className
    )}
    {...props}
  >
    <SliderPrimitive.Track className="relative h-2 w-full grow overflow-hidden rounded-full border border-brand-100/80 bg-brand-50 dark:border-white/10 dark:bg-white/10">
      <SliderPrimitive.Range className="absolute h-full bg-gradient-to-r from-brand-400 via-brand-500 to-brand-600 dark:from-indigo-500 dark:via-indigo-500 dark:to-primary" />
    </SliderPrimitive.Track>
    <SliderPrimitive.Thumb className="block h-0 w-0 opacity-0 pointer-events-none" />
  </SliderPrimitive.Root>
));
Slider.displayName = SliderPrimitive.Root.displayName;

export { Slider };
