"use client";

import { useRouter } from "next/navigation";
import { useEffect } from "react";

export default function PricingPage() {
  const router = useRouter();

  useEffect(() => {
    // Redirect to the pricing section on the landing page
    router.replace("/#pricing");
  }, [router]);

  return (
    <div className="min-h-screen flex items-center justify-center">
      <div className="text-muted-foreground">Redirecting to pricing...</div>
    </div>
  );
}
