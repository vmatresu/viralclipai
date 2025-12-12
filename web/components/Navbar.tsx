"use client";

import { DollarSign, History, LogOut, Menu, Settings, Sparkles } from "lucide-react";
import Link from "next/link";
import * as React from "react";

import { SignInDialog } from "@/components/SignInDialog";
import { ThemeSwitcher } from "@/components/theme-switcher";
import { Button } from "@/components/ui/button";
import {
    Sheet,
    SheetContent,
    SheetHeader,
    SheetTitle,
    SheetTrigger,
} from "@/components/ui/sheet";
import { useAuth } from "@/lib/auth";

// Navigation configuration - DRY principle
const NAV_LINKS = [
  { href: "/analyze", label: "Analyze", icon: Sparkles },
  { href: "/pricing", label: "Pricing", icon: DollarSign },
  { href: "/history", label: "History", icon: History },
  { href: "/settings", label: "Settings", icon: Settings },
] as const;

// Mobile navigation component - Single Responsibility
function MobileNav() {
  const { user, loading, signOut } = useAuth();
  const [open, setOpen] = React.useState(false);

  const handleNavClick = () => {
    setOpen(false);
  };

  const handleSignOut = async () => {
    await signOut();
    setOpen(false);
  };

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="sm:hidden h-9 w-9"
          aria-label="Open menu"
        >
          <Menu className="h-5 w-5" />
        </Button>
      </SheetTrigger>
      <SheetContent side="right" className="glass w-[280px] sm:w-[320px]">
        <SheetHeader className="text-left">
          <SheetTitle className="flex items-center gap-2">
            <img
              src="/logo.svg"
              alt="Viral Clip AI"
              width={24}
              height={24}
              className="w-6 h-6"
            />
            <span className="bg-clip-text text-transparent bg-gradient-to-r from-brand-500 to-brand-700">
              Viral Clip AI
            </span>
          </SheetTitle>
        </SheetHeader>

        <nav className="flex flex-col gap-1 mt-8">
          {NAV_LINKS.map((link) => {
            const Icon = link.icon;
            return (
              <Link
                key={link.href}
                href={link.href}
                onClick={handleNavClick}
                className="flex items-center gap-3 px-3 py-3 rounded-lg text-sm font-medium transition-colors hover:bg-accent hover:text-accent-foreground"
              >
                <Icon className="h-5 w-5" />
                <span>{link.label}</span>
              </Link>
            );
          })}
        </nav>

        <div className="absolute bottom-6 left-6 right-6 space-y-4">
          {!loading && !user && (
            <div onClick={handleNavClick}>
              <SignInDialog />
            </div>
          )}
          {!loading && user && (
            <div className="space-y-3">
              <p className="text-xs text-muted-foreground truncate px-1">
                {user.email}
              </p>
              <Button
                onClick={handleSignOut}
                variant="outline"
                className="w-full gap-2"
              >
                <LogOut className="h-4 w-4" />
                Sign out
              </Button>
            </div>
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}

// Desktop navigation links - Single Responsibility
function DesktopNav() {
  return (
    <>
      {NAV_LINKS.map((link) => {
        const Icon = link.icon;
        return (
          <Button
            key={link.href}
            asChild
            variant="ghost"
            size="sm"
            className="hidden sm:flex"
          >
            <Link href={link.href} className="gap-2">
              <Icon className="h-4 w-4" />
              <span>{link.label}</span>
            </Link>
          </Button>
        );
      })}
    </>
  );
}

// Main Navbar component - Composition pattern
export function Navbar() {
  const { user, loading, signOut } = useAuth();

  return (
    <nav className="glass fixed top-0 w-full z-50 border-b">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex items-center justify-between h-16">
          {/* Logo */}
          <div className="flex items-center gap-3">
            <Link
              href="/"
              className="flex items-center gap-3 hover:opacity-80 transition-opacity"
            >
              <img
                src="/logo.svg"
                alt="Viral Clip AI Logo"
                width={32}
                height={32}
                className="w-8 h-8"
              />
              <span className="text-xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-brand-500 to-brand-700">
                Viral Clip AI
              </span>
            </Link>
          </div>

          {/* Desktop Navigation + Actions */}
          <div className="flex items-center gap-2">
            <DesktopNav />
            <ThemeSwitcher />

            {/* Desktop Auth */}
            {!loading && !user && (
              <div className="hidden sm:block">
                <SignInDialog />
              </div>
            )}
            {!loading && user && (
              <div className="hidden sm:flex items-center gap-2">
                <span className="hidden md:inline text-xs text-muted-foreground max-w-[150px] truncate">
                  {user.email}
                </span>
                <Button onClick={signOut} variant="ghost" size="sm" className="gap-2">
                  <LogOut className="h-4 w-4" />
                  <span className="hidden sm:inline">Sign out</span>
                </Button>
              </div>
            )}

            {/* Mobile Menu */}
            <MobileNav />
          </div>
        </div>
      </div>
    </nav>
  );
}
