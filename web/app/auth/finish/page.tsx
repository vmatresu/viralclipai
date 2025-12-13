"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState, Suspense } from "react";
import { toast } from "sonner";

import { useAuth } from "@/lib/auth";

function AuthFinishContent() {
  const router = useRouter();
  const { isEmailLink, finishEmailSignIn } = useAuth();
  const [verifying, setVerifying] = useState(true);

  useEffect(() => {
    const handleFinish = async () => {
      const link = window.location.href;

      if (isEmailLink(link)) {
        let email = window.localStorage.getItem("emailForSignIn");

        // User opened link on different device - prompt is necessary for cross-device sign-in
        // eslint-disable-next-line no-alert, @typescript-eslint/prefer-nullish-coalescing
        email ||= window.prompt("Please provide your email for confirmation");

        if (!email) {
          toast.error("Email is required to complete sign in.");
          setVerifying(false);
          return;
        }

        try {
          await finishEmailSignIn(email, link);
          toast.success("Successfully signed in!");
          router.push("/"); // Redirect to dashboard
        } catch (error: unknown) {
          const message =
            error instanceof Error ? error.message : "Failed to complete sign in.";
          toast.error(message);
          setVerifying(false);
        }
      } else {
        setVerifying(false);
        router.push("/");
      }
    };

    void handleFinish();
  }, [isEmailLink, finishEmailSignIn, router]);

  if (verifying) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[60vh]">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-brand-500 mb-4" />
        <p className="text-lg text-muted-foreground">Verifying your sign in...</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh]">
      <p className="text-lg text-red-500">
        Invalid sign-in link or verification failed.
      </p>
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
