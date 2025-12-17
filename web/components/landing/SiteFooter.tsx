"use client";

import Link from "next/link";

import { Logo } from "./Logo";

const FOOTER_LINKS = [
  { label: "Product", href: "/" },
  { label: "Pricing", href: "#pricing" },
  { label: "History", href: "/history" },
  { label: "Settings", href: "/settings" },
  { label: "Terms", href: "/terms" },
  { label: "Privacy", href: "/privacy" },
];

export function SiteFooter() {
  const currentYear = new Date().getFullYear();

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
    }
  };

  return (
    <footer className="border-t border-border dark:border-white/5 bg-background dark:bg-brand-darker/50 backdrop-blur-xl">
      <div className="landing-container py-12">
        <div className="flex flex-col md:flex-row items-center justify-between gap-8 mb-8">
          {/* Brand */}
          <div className="opacity-80 hover:opacity-100 transition-opacity">
            <Logo />
          </div>

          {/* Links */}
          <nav className="flex flex-wrap justify-center gap-x-8 gap-y-3">
            {FOOTER_LINKS.map((link) => (
              <Link
                key={link.label}
                href={link.href}
                onClick={(e) => handleNavClick(e, link.href)}
                className="text-sm text-muted-foreground hover:text-foreground transition-colors"
              >
                {link.label}
              </Link>
            ))}
          </nav>
        </div>

        {/* Copyright */}
        <div className="pt-8 border-t border-border dark:border-white/5 text-center">
          <p className="text-sm text-muted-foreground/60">
            © {currentYear} Viral Clip AI. All rights reserved.
          </p>
        </div>
      </div>
    </footer>
  );
}
