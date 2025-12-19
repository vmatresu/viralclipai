"use client";

import { ChevronDown, CreditCard, LogOut, Menu, Settings, X } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";

import { SignInDialog } from "@/components/SignInDialog";
import { ThemeToggle } from "@/components/ThemeToggle";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useAuth } from "@/lib/auth";
import { cn } from "@/lib/utils";

import { Logo } from "./Logo";

const NAV_LINKS = [
  { href: "#how-it-works", label: "How it works" },
  { href: "#for-creators", label: "For creators" },
  { href: "#pricing", label: "Pricing" },
] as const;

export function SiteHeader() {
  const { user, loading, signOut } = useAuth();
  const [isScrolled, setIsScrolled] = useState(false);
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  useEffect(() => {
    const handleScroll = () => {
      setIsScrolled(window.scrollY > 20);
    };
    window.addEventListener("scroll", handleScroll, { passive: true });
    return () => window.removeEventListener("scroll", handleScroll);
  }, []);

  const handleNavClick = (e: React.MouseEvent<HTMLAnchorElement>, href: string) => {
    if (href.startsWith("#")) {
      e.preventDefault();
      const target = document.querySelector(href);
      if (target) {
        const navHeight = 80;
        const targetPosition =
          target.getBoundingClientRect().top + window.scrollY - navHeight - 20;
        window.scrollTo({ top: targetPosition, behavior: "smooth" });
      }
      setIsMobileMenuOpen(false);
    }
  };

  return (
    <nav
      className={cn(
        "fixed top-0 left-0 right-0 z-50 transition-all duration-300",
        isScrolled
          ? "h-20 bg-transparent backdrop-blur-lg border-b border-border/40 dark:border-white/5"
          : "h-24 bg-transparent"
      )}
    >
      <div className="landing-container max-w-5xl h-full relative flex items-center mx-auto">
        {/* Logo */}
        <div className="mr-auto">
          <Logo />
        </div>

        {/* Desktop Nav Links - Centered */}
        <div className="hidden md:flex items-center gap-8 absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2">
          {NAV_LINKS.map((link) => (
            <a
              key={link.href}
              href={link.href}
              onClick={(e) => handleNavClick(e, link.href)}
              className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors relative group"
            >
              {link.label}
              <span className="absolute -bottom-1 left-0 w-0 h-0.5 bg-gradient-to-r from-brand-400 to-brand-cyan transition-all duration-300 group-hover:w-full" />
            </a>
          ))}
          {!loading && user && (
            <Link
              href="/credits"
              className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors relative group"
            >
              Credits
              <span className="absolute -bottom-1 left-0 w-0 h-0.5 bg-gradient-to-r from-brand-400 to-brand-cyan transition-all duration-300 group-hover:w-full" />
            </Link>
          )}
        </div>

        {/* Desktop Actions */}
        <div className="hidden md:flex items-center gap-4 ml-auto">
          {!loading && !user && (
            <>
              <SignInDialog>
                <Button
                  variant="ghost"
                  className="text-sm font-medium text-muted-foreground hover:text-foreground"
                >
                  Log in
                </Button>
              </SignInDialog>
              <Button
                asChild
                className="btn-primary-gradient text-sm font-semibold px-5 py-2 rounded-lg"
              >
                <Link href="/#process-video">Generate My First Clip</Link>
              </Button>
            </>
          )}
          <ThemeToggle />
          {!loading && user && (
            <>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    className="gap-2 pl-2 pr-3 h-10 rounded-full hover:bg-accent/50 dark:hover:bg-white/10 transition-colors"
                  >
                    <div className="h-8 w-8 rounded-full bg-gradient-to-tr from-brand-400 to-brand-cyan p-[1px]">
                      <div className="h-full w-full rounded-full bg-[#05060D] flex items-center justify-center">
                        <span className="text-xs font-bold text-white">
                          {user.email?.[0]?.toUpperCase() ?? "U"}
                        </span>
                      </div>
                    </div>
                    <span className="hidden lg:inline text-sm font-medium text-foreground dark:text-white/90 max-w-[150px] truncate">
                      {user.email?.split("@")[0]}
                    </span>
                    <ChevronDown className="h-4 w-4 text-muted-foreground" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent
                  align="end"
                  className="w-56 bg-white dark:bg-[#0B0E1A] border-gray-200 dark:border-white/10 text-gray-900 dark:text-gray-100 shadow-lg dark:shadow-none"
                >
                  <DropdownMenuLabel className="font-normal">
                    <div className="flex flex-col space-y-1">
                      <p className="text-sm font-medium leading-none">Account</p>
                      <p className="text-xs leading-none text-muted-foreground">
                        {user.email}
                      </p>
                    </div>
                  </DropdownMenuLabel>
                  <DropdownMenuSeparator className="bg-gray-200 dark:bg-white/10" />
                  <DropdownMenuItem asChild>
                    <Link href="/settings" className="cursor-pointer w-full">
                      <Settings className="mr-2 h-4 w-4" />
                      <span>Settings</span>
                    </Link>
                  </DropdownMenuItem>
                  <DropdownMenuItem asChild>
                    <Link href="/credits" className="cursor-pointer w-full">
                      <CreditCard className="mr-2 h-4 w-4" />
                      <span>Credits</span>
                    </Link>
                  </DropdownMenuItem>
                  <DropdownMenuSeparator className="bg-gray-200 dark:bg-white/10" />
                  <DropdownMenuItem
                    onClick={signOut}
                    className="text-red-600 dark:text-red-400 focus:text-red-600 dark:focus:text-red-400 focus:bg-red-50 dark:focus:bg-red-400/10 cursor-pointer"
                  >
                    <LogOut className="mr-2 h-4 w-4" />
                    <span>Sign out</span>
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <Button
                asChild
                className="btn-primary-gradient text-sm font-semibold px-5 py-2 rounded-lg"
              >
                <Link href="/history">View History</Link>
              </Button>
            </>
          )}
        </div>

        {/* Mobile Menu Toggle */}
        <button
          onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
          className="md:hidden ml-auto p-2 text-foreground"
          aria-label="Toggle navigation menu"
          aria-expanded={isMobileMenuOpen}
        >
          {isMobileMenuOpen ? <X className="w-6 h-6" /> : <Menu className="w-6 h-6" />}
        </button>
      </div>

      {/* Mobile Menu */}
      <div
        className={cn(
          "md:hidden fixed top-20 left-0 right-0 bg-background border-b border-border transition-all duration-300",
          isMobileMenuOpen
            ? "opacity-100 translate-y-0"
            : "opacity-0 -translate-y-4 pointer-events-none"
        )}
      >
        <div className="px-6 py-6 space-y-4">
          {NAV_LINKS.map((link) => (
            <a
              key={link.href}
              href={link.href}
              onClick={(e) => handleNavClick(e, link.href)}
              className="block text-lg font-medium text-muted-foreground hover:text-foreground py-2 border-b border-border"
            >
              {link.label}
            </a>
          ))}
          {!loading && user && (
            <Link
              href="/credits"
              className="block text-lg font-medium text-muted-foreground hover:text-foreground py-2 border-b border-border"
            >
              Credits
            </Link>
          )}
          <div className="flex items-center justify-between py-2 border-b border-border">
            <span className="text-sm font-medium text-muted-foreground">Theme</span>
            <ThemeToggle />
          </div>
          {!loading && !user && (
            <div className="pt-2">
              <SignInDialog />
            </div>
          )}
          {!loading && user && (
            <div className="pt-2 space-y-3">
              <p className="text-xs text-muted-foreground truncate">{user.email}</p>
              <Button onClick={signOut} variant="outline" className="w-full gap-2">
                <LogOut className="h-4 w-4" />
                Sign out
              </Button>
            </div>
          )}
          <Button
            asChild
            className="w-full btn-primary-gradient text-sm font-semibold py-3 rounded-lg mt-4"
          >
            <Link href="/history">View History</Link>
          </Button>
        </div>
      </div>
    </nav>
  );
}
