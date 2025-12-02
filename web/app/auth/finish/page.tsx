"use client";

import { useEffect, useState, Suspense } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useAuth } from "@/lib/auth";
import { toast } from "sonner";

function AuthFinishContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const { isEmailLink, finishEmailSignIn } = useAuth();
  const [verifying, setVerifying] = useState(true);

  useEffect(() => {
    const handleFinish = async () => {
      const link = window.location.href;
      
      if (isEmailLink(link)) {
        let email = window.localStorage.getItem('emailForSignIn');
        
        if (!email) {
          // User opened link on different device
          email = window.prompt('Please provide your email for confirmation');
        }
        
        if (!email) {
            toast.error("Email is required to complete sign in.");
            setVerifying(false);
            return;
        }

        try {
          await finishEmailSignIn(email, link);
          toast.success("Successfully signed in!");
          router.push("/"); // Redirect to dashboard
        } catch (error: any) {
          toast.error(error.message || "Failed to complete sign in.");
          setVerifying(false);
        }
      } else {
        setVerifying(false);
        router.push("/");
      }
    };

    handleFinish();
  }, [isEmailLink, finishEmailSignIn, router]);

  if (verifying) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[60vh]">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-brand-500 mb-4"></div>
        <p className="text-lg text-muted-foreground">Verifying your sign in...</p>
      </div>
    );
  }

  return (
      <div className="flex flex-col items-center justify-center min-h-[60vh]">
          <p className="text-lg text-red-500">Invalid sign-in link or verification failed.</p>
      </div>
  );
}

export default function AuthFinishPage() {
  return (
    <Suspense fallback={<div>Loading...</div>}>
      <AuthFinishContent />
    </Suspense>
  );
}
