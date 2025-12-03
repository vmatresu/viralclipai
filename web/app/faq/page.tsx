"use client";

import { HelpCircle, Video, Zap, Shield, DollarSign, Settings } from "lucide-react";
import Link from "next/link";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { usePageView } from "@/lib/usePageView";

export default function FAQPage() {
  usePageView("faq");

  const faqCategories = [
    {
      title: "Getting Started",
      icon: Zap,
      questions: [
        {
          q: "How do I get started?",
          a: "Sign in with your Google account, paste a YouTube URL on the home page, choose your output style, and wait for the AI to analyze and generate clips. It's that simple!",
        },
        {
          q: "Which videos work best?",
          a: "Commentary, reaction, educational, and podcast-style videos where you speak throughout usually produce the strongest highlights. Videos with clear dialogue and engaging moments work best.",
        },
        {
          q: "Can I use non-YouTube sources?",
          a: "Currently we focus on YouTube URLs. If your source can be imported into YouTube (public or unlisted), it will likely work well. We're always working on expanding support for other platforms.",
        },
        {
          q: "How long does processing take?",
          a: "Processing time depends on video length and the number of styles selected. Typically, a 10-minute video takes 2-5 minutes to analyze and generate clips. Intelligent styles may take slightly longer due to AI processing.",
        },
      ],
    },
    {
      title: "Video Styles & Formats",
      icon: Video,
      questions: [
        {
          q: "What's the difference between styles?",
          a: "Styles determine how your video is cropped and formatted. Split View shows both sides stacked vertically, Left/Right Focus crops to one side, Intelligent Crop uses AI to track faces, and Original keeps your video unchanged. See our Styles page for detailed explanations.",
        },
        {
          q: "Which style should I use for TikTok?",
          a: "Intelligent Split View is specifically optimized for TikTok and other 9:16 platforms. It guarantees the correct format and uses AI to keep subjects perfectly framed.",
        },
        {
          q: "Can I generate multiple styles at once?",
          a: "Yes! Select 'All Styles' to generate every available style, or select multiple individual styles. Each style creates a separate output file.",
        },
        {
          q: "Do styles affect video quality?",
          a: "Styles use high-quality processing to minimize quality loss. However, any cropping and resizing can slightly reduce quality. The intelligent styles are optimized to preserve as much quality as possible.",
        },
      ],
    },
    {
      title: "Plans & Pricing",
      icon: DollarSign,
      questions: [
        {
          q: "How do plans work?",
          a: "Each user has a plan (Free or Pro) that controls how many clips can be generated per calendar month. The backend checks your monthly usage before starting a new job and returns an error if you're over quota.",
        },
        {
          q: "How do I upgrade or cancel?",
          a: "Plan management is tied to your account. Today upgrades are managed manually—contact us via the Contact page and we'll help you upgrade or make changes to your plan.",
        },
        {
          q: "What happens if I exceed my limit?",
          a: "If you exceed your monthly clip limit, you'll receive an error message when trying to process a new video. Your limit resets at the beginning of each calendar month.",
        },
        {
          q: "Can I see my usage?",
          a: "Yes! Check your Settings page to see your current plan, monthly usage, and remaining clips for the month.",
        },
      ],
    },
    {
      title: "Privacy & Security",
      icon: Shield,
      questions: [
        {
          q: "Do you keep my videos forever?",
          a: "Raw source videos are used as temporary working files and then removed. Clips and thumbnails are stored in S3 under your account until you delete them or request deletion.",
        },
        {
          q: "How is my data secured?",
          a: "We use Firebase Auth for authentication, Firestore for user data, and S3 for media storage. Each user's assets are stored under a per-user prefix with proper isolation. Downloads use short-lived presigned URLs.",
        },
        {
          q: "Who can see my videos?",
          a: "Only you can see your videos. All data is isolated per user ID, and we don't share your content with anyone else. Your clips are private to your account.",
        },
        {
          q: "Can I delete my data?",
          a: "Yes, you can delete individual clips from your history page. For complete account deletion, please contact us through the Contact page.",
        },
      ],
    },
    {
      title: "Technical",
      icon: Settings,
      questions: [
        {
          q: "What video formats are supported?",
          a: "We support YouTube videos in standard formats. The output clips are in MP4 format, optimized for social media platforms.",
        },
        {
          q: "Can I customize the AI prompts?",
          a: "Yes! You can provide a custom prompt when processing videos to guide the AI in finding specific types of moments or content.",
        },
        {
          q: "How does intelligent cropping work?",
          a: "Intelligent cropping uses computer vision to detect faces and important subjects frame-by-frame, then dynamically adjusts the crop window to keep them centered and in focus throughout the video.",
        },
        {
          q: "Can I publish directly to TikTok?",
          a: "Yes, if you've configured your TikTok credentials in Settings. After clips are generated, you can publish them directly to TikTok without leaving the platform.",
        },
      ],
    },
  ];

  return (
    <div className="space-y-12">
      <section className="space-y-4">
        <div className="flex items-center gap-3">
          <HelpCircle className="h-8 w-8 text-primary" />
          <h1 className="text-3xl md:text-4xl font-extrabold">
            Frequently Asked Questions
          </h1>
        </div>
        <p className="text-muted-foreground max-w-3xl text-lg">
          Find answers to common questions about Viral Clip AI. Can't find what you're
          looking for?{" "}
          <Link href="/contact" className="text-primary hover:underline">
            Contact us
          </Link>
          .
        </p>
      </section>

      {faqCategories.map((category) => {
        const Icon = category.icon;
        return (
          <section key={category.title} className="space-y-4">
            <div className="flex items-center gap-2">
              <Icon className="h-6 w-6 text-primary" />
              <h2 className="text-2xl font-bold">{category.title}</h2>
            </div>
            <div className="space-y-4">
              {category.questions.map((item, idx) => (
                <Card key={idx} className="glass">
                  <CardHeader>
                    <CardTitle className="text-lg">{item.q}</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <p className="text-muted-foreground">{item.a}</p>
                  </CardContent>
                </Card>
              ))}
            </div>
          </section>
        );
      })}

      <section className="space-y-4">
        <h2 className="text-2xl font-bold">Still Have Questions?</h2>
        <p className="text-muted-foreground">
          We're here to help! Reach out through our{" "}
          <Link href="/contact" className="text-primary hover:underline">
            Contact page
          </Link>{" "}
          or check out our{" "}
          <Link href="/docs" className="text-primary hover:underline">
            Documentation
          </Link>{" "}
          for more detailed information.
        </p>
      </section>
    </div>
  );
}
