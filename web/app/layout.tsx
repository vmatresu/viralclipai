import { Inter } from "next/font/google";
import { type ReactNode } from "react";
import { Toaster } from "sonner";

import { ClientProviders } from "@/components/ClientProviders";
import { SiteFooter } from "@/components/landing/SiteFooter";
import { SiteHeader } from "@/components/landing/SiteHeader";
import { AuthProvider } from "@/lib/auth";
import { ThemeProvider } from "@/lib/theme-provider";

import type { Metadata } from "next";

import "./globals.css";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-inter",
  weight: ["300", "400", "500", "600", "700"],
});

export const metadata: Metadata = {
  title:
    "Viral Clip AI - Creator Momentum Engine | Become a Daily Creator Without Editing",
  description:
    "Become a daily creator without editing. Turn long videos into viral-ready clips automatically and grow consistently without burnout.",
  keywords:
    "AI video clips, viral clips, content creator, YouTube clips, TikTok, Reels, Shorts, video editing AI",
  metadataBase: new URL("https://www.viralclipai.io"),
  alternates: {
    canonical: "/",
  },
  icons: {
    icon: "/logo.svg",
    shortcut: "/logo.svg",
    apple: "/logo.svg",
  },
  openGraph: {
    title: "Viral Clip AI - Creator Momentum Engine",
    description:
      "Become a daily creator without editing. Turn long videos into viral-ready clips automatically.",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
  },
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning className="scroll-smooth">
      <body
        className={`min-h-screen font-sans antialiased overflow-x-hidden ${inter.variable}`}
      >
        <ThemeProvider
          attribute="class"
          defaultTheme="dark"
          enableSystem={false}
          disableTransitionOnChange={false}
        >
          <AuthProvider>
            <ClientProviders>
              <SiteHeader />
              <main>{children}</main>
              <SiteFooter />
              <Toaster position="top-center" richColors />
            </ClientProviders>
          </AuthProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
