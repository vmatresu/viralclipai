"use client";

import { ArrowRight, LogIn, Mail } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAuth } from "@/lib/auth";

import { GoogleSignInButton } from "./GoogleSignInButton";

export function SignInDialog({ children }: { children?: React.ReactNode }) {
  const [email, setEmail] = useState("");
  const [isEmailLoading, setIsEmailLoading] = useState(false);
  const [isGoogleLoading, setIsGoogleLoading] = useState(false);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const { signInWithGoogle, sendEmailLink } = useAuth();

  const handleGoogleSignIn = async () => {
    setIsGoogleLoading(true);
    try {
      await signInWithGoogle();
      setIsDialogOpen(false);
      toast.success("Signed in with Google!");
    } catch (error: unknown) {
      const message =
        error instanceof Error ? error.message : "Failed to sign in with Google";
      toast.error(message);
    } finally {
      setIsGoogleLoading(false);
    }
  };

  const handleEmailSignIn = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email) {
      toast.error("Please enter your email address");
      return;
    }

    setIsEmailLoading(true);
    try {
      await sendEmailLink(email);
      toast.success("Sign-in link sent! Check your email.");
      setIsDialogOpen(false);
      setEmail("");
    } catch (error: unknown) {
      const message =
        error instanceof Error ? error.message : "Failed to send sign-in link";
      toast.error(message);
    } finally {
      setIsEmailLoading(false);
    }
  };

  return (
    <Dialog open={isDialogOpen} onOpenChange={setIsDialogOpen}>
      <DialogTrigger asChild>
        {children ?? (
          <Button
            variant="brand"
            size="sm"
            className="gap-2 shadow-md hover:shadow-lg transition-all"
          >
            <LogIn className="h-4 w-4" />
            <span className="hidden sm:inline">Get Started</span>
          </Button>
        )}
      </DialogTrigger>
      <DialogContent className="sm:max-w-[400px] p-0 overflow-hidden gap-0 bg-card border-border/50 shadow-2xl">
        <DialogHeader className="p-6 pb-2 text-center">
          <div className="mx-auto w-12 h-12 bg-gradient-to-br from-brand-100 to-brand-200 dark:from-brand-900/50 dark:to-brand-800/50 rounded-full flex items-center justify-center mb-4 shadow-inner border border-brand-200 dark:border-brand-700">
            <LogIn className="h-6 w-6 text-brand-600 dark:text-brand-400" />
          </div>
          <DialogTitle className="text-2xl font-bold tracking-tight">
            Welcome Back
          </DialogTitle>
          <DialogDescription className="text-base">
            Sign in to your account to continue
          </DialogDescription>
        </DialogHeader>

        <div className="p-6 pt-2 grid gap-6">
          <GoogleSignInButton
            onClick={handleGoogleSignIn}
            loading={isGoogleLoading}
            className="w-full h-11 text-base shadow-sm"
          />

          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t border-border" />
            </div>
            <div className="relative flex justify-center text-xs uppercase">
              <span className="bg-card px-3 text-muted-foreground font-medium tracking-wider">
                Or with email
              </span>
            </div>
          </div>

          <form onSubmit={handleEmailSignIn} className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="email" className="sr-only">
                Email address
              </Label>
              <div className="relative">
                <div className="absolute left-3 top-3 text-muted-foreground">
                  <Mail className="h-5 w-5" />
                </div>
                <Input
                  id="email"
                  placeholder="name@example.com"
                  type="email"
                  autoCapitalize="none"
                  autoComplete="email"
                  autoCorrect="off"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  disabled={isEmailLoading}
                  className="pl-10 h-11"
                />
              </div>
            </div>
            <Button
              type="submit"
              className="w-full h-11 gap-2 font-semibold shadow-md"
              disabled={isEmailLoading}
              variant="default"
            >
              {isEmailLoading ? (
                <span className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
              ) : (
                <>
                  Continue with Email
                  <ArrowRight className="h-4 w-4 ml-1" />
                </>
              )}
            </Button>
          </form>
        </div>
        <div className="p-4 bg-muted/30 text-center text-xs text-muted-foreground border-t border-border/50">
          By continuing, you agree to our Terms of Service and Privacy Policy.
        </div>
      </DialogContent>
    </Dialog>
  );
}
