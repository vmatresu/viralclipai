"use client";

import {
  BookOpen,
  Rocket,
  Shield,
  Settings,
  Video,
  FileText,
  Link as LinkIcon,
} from "lucide-react";
import Link from "next/link";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { usePageView } from "@/lib/usePageView";

export default function DocsPage() {
  usePageView("docs");

  const categories = [
    {
      title: "Getting Started",
      icon: Rocket,
      description: "Learn the basics and get up and running quickly",
      items: [
        {
          title: "Quick Start Guide",
          description: "Step-by-step instructions to process your first video",
          href: "#getting-started",
        },
        {
          title: "Video Styles Explained",
          description: "Understand how different styles transform your videos",
          href: "/styles",
        },
        {
          title: "Best Practices",
          description: "Tips for getting the best results from your videos",
          href: "#best-practices",
        },
      ],
    },
    {
      title: "Features",
      icon: Video,
      description: "Explore all the features and capabilities",
      items: [
        {
          title: "Video Styles",
          description: "Split View, Intelligent Crop, and more style options",
          href: "/styles",
        },
        {
          title: "AI Highlight Detection",
          description: "How our AI finds the most engaging moments",
          href: "#ai-detection",
        },
        {
          title: "TikTok Publishing",
          description: "Publish clips directly to TikTok from the platform",
          href: "#tiktok-publishing",
        },
      ],
    },
    {
      title: "Account & Plans",
      icon: Settings,
      description: "Manage your account, plans, and usage",
      items: [
        {
          title: "Plans & Pricing",
          description: "Understand different plan tiers and limits",
          href: "/pricing",
        },
        {
          title: "Usage & Limits",
          description: "How monthly limits work and what happens when exceeded",
          href: "#usage-limits",
        },
        {
          title: "Account Settings",
          description: "Manage your account preferences and integrations",
          href: "/settings",
        },
      ],
    },
    {
      title: "Security & Privacy",
      icon: Shield,
      description: "How we protect your data and content",
      items: [
        {
          title: "Data Storage",
          description: "Where your videos are stored and how long we keep them",
          href: "#data-storage",
        },
        {
          title: "Privacy Policy",
          description: "How we handle your personal information",
          href: "#privacy",
        },
        {
          title: "Security Practices",
          description: "Authentication, encryption, and data isolation",
          href: "#security",
        },
      ],
    },
    {
      title: "Help & Support",
      icon: FileText,
      description: "Get help and find answers",
      items: [
        {
          title: "FAQ",
          description: "Frequently asked questions and answers",
          href: "/faq",
        },
        {
          title: "Contact Us",
          description: "Get in touch with our support team",
          href: "/contact",
        },
        {
          title: "Troubleshooting",
          description: "Common issues and how to resolve them",
          href: "#troubleshooting",
        },
      ],
    },
  ];

  return (
    <div className="space-y-12">
      <section className="space-y-4">
        <div className="flex items-center gap-3">
          <BookOpen className="h-8 w-8 text-primary" />
          <h1 className="text-3xl md:text-4xl font-extrabold">Documentation</h1>
        </div>
        <p className="text-muted-foreground max-w-3xl text-lg">
          Everything you need to know about Viral Clip AI. Browse by category or use the
          search to find specific topics.
        </p>
      </section>

      {/* Categories Grid */}
      <section className="space-y-6">
        <h2 className="text-2xl font-bold">Browse by Category</h2>
        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-6">
          {categories.map((category) => {
            const Icon = category.icon;
            return (
              <Card key={category.title} className="glass">
                <CardHeader>
                  <div className="flex items-center gap-2">
                    <Icon className="h-5 w-5 text-primary" />
                    <CardTitle>{category.title}</CardTitle>
                  </div>
                  <CardDescription>{category.description}</CardDescription>
                </CardHeader>
                <CardContent className="space-y-2">
                  {category.items.map((item) => (
                    <Link
                      key={item.title}
                      href={item.href}
                      className="block p-2 rounded-md hover:bg-accent transition-colors group"
                    >
                      <div className="flex items-start gap-2">
                        <LinkIcon className="h-4 w-4 text-muted-foreground mt-0.5 group-hover:text-primary transition-colors" />
                        <div>
                          <p className="font-medium text-sm group-hover:text-primary transition-colors">
                            {item.title}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {item.description}
                          </p>
                        </div>
                      </div>
                    </Link>
                  ))}
                </CardContent>
              </Card>
            );
          })}
        </div>
      </section>

      {/* Getting Started Section */}
      <section id="getting-started" className="space-y-4">
        <h2 className="text-2xl font-bold">Getting Started</h2>
        <Card className="glass">
          <CardContent className="pt-6">
            <ol className="list-decimal list-inside space-y-3 text-muted-foreground">
              <li>
                <span className="font-semibold text-foreground">Sign in</span> using
                your Google account (Firebase Auth)
              </li>
              <li>
                <span className="font-semibold text-foreground">
                  Paste a YouTube URL
                </span>{" "}
                on the Home page and choose an output style
              </li>
              <li>
                <span className="font-semibold text-foreground">
                  Wait for processing
                </span>{" "}
                - the AI will analyze your video and generate clips
              </li>
              <li>
                <span className="font-semibold text-foreground">
                  Download or publish
                </span>{" "}
                clips directly to TikTok (if configured)
              </li>
            </ol>
          </CardContent>
        </Card>
      </section>

      {/* Authentication Section */}
      <section id="security" className="space-y-4">
        <h2 className="text-2xl font-bold">Authentication & Security</h2>
        <Card className="glass">
          <CardContent className="pt-6 space-y-3 text-muted-foreground">
            <p>
              We use Firebase Auth on the frontend. The browser obtains a short-lived ID
              token which is sent to the FastAPI backend via WebSockets and HTTP
              headers. The backend verifies the token with Firebase Admin and isolates
              data by user ID.
            </p>
            <p>
              All user data is properly isolated, and downloads use short-lived
              presigned URLs for security.
            </p>
          </CardContent>
        </Card>
      </section>

      {/* Storage Section */}
      <section id="data-storage" className="space-y-4">
        <h2 className="text-2xl font-bold">Storage & Data</h2>
        <Card className="glass">
          <CardContent className="pt-6">
            <ul className="list-disc list-inside space-y-2 text-muted-foreground">
              <li>
                <span className="font-semibold text-foreground">S3 storage:</span>{" "}
                Clips, thumbnails, and analysis metadata are stored in S3
              </li>
              <li>
                <span className="font-semibold text-foreground">User isolation:</span>{" "}
                Each user&apos;s assets are stored under a per-user prefix
              </li>
              <li>
                <span className="font-semibold text-foreground">Secure downloads:</span>{" "}
                Downloads use short-lived presigned URLs
              </li>
              <li>
                <span className="font-semibold text-foreground">Firestore:</span> Tracks
                usage, history, and user settings (including TikTok tokens)
              </li>
              <li>
                <span className="font-semibold text-foreground">Temporary files:</span>{" "}
                Raw source videos are used as temporary working files and then removed
              </li>
            </ul>
          </CardContent>
        </Card>
      </section>

      {/* Limits Section */}
      <section id="usage-limits" className="space-y-4">
        <h2 className="text-2xl font-bold">Limits and Plans</h2>
        <Card className="glass">
          <CardContent className="pt-6 space-y-3 text-muted-foreground">
            <p>
              Each user has a plan (Free or Pro) that controls how many clips can be
              generated per calendar month. The backend checks your monthly usage before
              starting a new job and returns an error if you are over quota.
            </p>
            <p>
              See the{" "}
              <Link href="/pricing" className="text-primary hover:underline">
                Pricing page
              </Link>{" "}
              for detailed plan information and limits.
            </p>
          </CardContent>
        </Card>
      </section>

      {/* Quick Links */}
      <section className="space-y-4">
        <h2 className="text-2xl font-bold">Quick Links</h2>
        <div className="grid md:grid-cols-2 gap-4">
          <Card className="glass">
            <CardHeader>
              <CardTitle>Need Help?</CardTitle>
            </CardHeader>
            <CardContent>
              <Link href="/faq" className="text-primary hover:underline font-medium">
                Visit our FAQ page →
              </Link>
            </CardContent>
          </Card>
          <Card className="glass">
            <CardHeader>
              <CardTitle>Learn About Styles</CardTitle>
            </CardHeader>
            <CardContent>
              <Link href="/styles" className="text-primary hover:underline font-medium">
                Explore video styles →
              </Link>
            </CardContent>
          </Card>
        </div>
      </section>
    </div>
  );
}
