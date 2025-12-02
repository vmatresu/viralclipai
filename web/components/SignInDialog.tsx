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
import { useState } from "react";
import { toast } from "sonner";
import { GoogleSignInButton } from "./GoogleSignInButton";
import { LogIn, Mail } from "lucide-react";

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
    } catch (error: any) {
      toast.error(error.message || "Failed to sign in with Google");
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
    } catch (error: any) {
      toast.error(error.message || "Failed to send sign-in link");
    } finally {
      setIsEmailLoading(false);
    }
  };

  return (
    <Dialog open={isDialogOpen} onOpenChange={setIsDialogOpen}>
      <DialogTrigger asChild>
        {children || (
          <Button variant="ghost" size="sm" className="gap-2">
            <LogIn className="h-4 w-4" />
            <span className="hidden sm:inline">Sign in</span>
          </Button>
        )}
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Sign in to Viral Clip AI</DialogTitle>
          <DialogDescription>
            Choose your preferred sign-in method below.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col space-y-4 py-4">
          <GoogleSignInButton 
            onClick={handleGoogleSignIn} 
            loading={isGoogleLoading}
            className="w-full"
          />
          
          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t" />
            </div>
            <div className="relative flex justify-center text-xs uppercase">
              <span className="bg-background px-2 text-muted-foreground">
                Or continue with email
              </span>
            </div>
          </div>

          <form onSubmit={handleEmailSignIn} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="email">Email address</Label>
              <Input
                id="email"
                placeholder="name@example.com"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                disabled={isEmailLoading}
              />
            </div>
            <Button 
              type="submit" 
              className="w-full gap-2" 
              disabled={isEmailLoading}
            >
              {isEmailLoading ? (
                  <span className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
              ) : (
                  <Mail className="h-4 w-4" />
              )}
              Send Sign-in Link
            </Button>
          </form>
        </div>
      </DialogContent>
    </Dialog>
  );
}
