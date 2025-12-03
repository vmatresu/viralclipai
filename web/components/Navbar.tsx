"use client";

import {
  Home,
  DollarSign,
  BookOpen,
  Info,
  Mail,
  History,
  Settings,
  LogIn,
  LogOut,
} from "lucide-react";
import Link from "next/link";

import { SignInDialog } from "@/components/SignInDialog";
import { ThemeSwitcher } from "@/components/theme-switcher";
import { Button } from "@/components/ui/button";
import { useAuth } from "@/lib/auth";

const navLinks = [
  { href: "/", label: "Home", icon: Home },
  { href: "/pricing", label: "Pricing", icon: DollarSign },
  { href: "/docs", label: "Docs", icon: BookOpen },
  { href: "/about", label: "About", icon: Info },
  { href: "/contact", label: "Contact", icon: Mail },
  { href: "/history", label: "History", icon: History },
  { href: "/settings", label: "Settings", icon: Settings },
];

export function Navbar() {
  const { user, loading, signOut } = useAuth();

  return (
    <nav className="glass fixed top-0 w-full z-50 border-b">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex items-center justify-between h-16">
          <div className="flex items-center gap-3">
            <Link href="/" className="flex items-center gap-3 hover:opacity-80 transition-opacity">
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
          <div className="flex items-center gap-2">
            {navLinks.map((link) => {
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
            <ThemeSwitcher />
            {!loading && !user && (
              <SignInDialog />
            )}
            {!loading && user && (
              <div className="flex items-center gap-2">
                <span className="hidden md:inline text-xs text-muted-foreground max-w-[150px] truncate">
                  {user.email}
                </span>
                <Button onClick={signOut} variant="ghost" size="sm" className="gap-2">
                  <LogOut className="h-4 w-4" />
                  <span className="hidden sm:inline">Sign out</span>
                </Button>
              </div>
            )}
          </div>
        </div>
      </div>
    </nav>
  );
}
