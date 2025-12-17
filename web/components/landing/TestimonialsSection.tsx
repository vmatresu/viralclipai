"use client";

import { GlassCard } from "./GlassCard";
import { LandingSection, SectionHeader } from "./LandingSection";

const TESTIMONIALS = [
  {
    quote:
      "I went from posting twice a month to daily. All my growth happened after I started using Viral Clip AI.",
    author: "Jake, content creator",
    tagline: "From sporadic to daily posting",
    initial: "J",
  },
  {
    quote: "Finally a tool that removes burnout instead of adding more work.",
    author: "Emily, business coach",
    tagline: "More visibility, less stress",
    initial: "E",
  },
  {
    quote: "I don't edit anymore — and my views have doubled.",
    author: "PodcastLab",
    tagline: "Podcast → clips → growth",
    initial: "P",
  },
];

export function TestimonialsSection() {
  return (
    <LandingSection
      id="social-proof"
      className="bg-gradient-to-b from-transparent via-brand-cyan/[0.02] to-transparent"
    >
      <SectionHeader
        title={
          <>
            Creators are growing faster{" "}
            <span className="gradient-text">with less effort.</span>
          </>
        }
      />

      <div className="grid md:grid-cols-3 gap-6">
        {TESTIMONIALS.map((testimonial) => (
          <GlassCard
            key={testimonial.author}
            className="hover:border-brand-cyan/25 hover:shadow-[0_25px_60px_rgba(0,0,0,0.4),0_0_40px_rgba(92,255,249,0.1)]"
          >
            {/* Quote icon */}
            <svg className="w-8 h-8 mb-4 opacity-50" viewBox="0 0 24 24" fill="none">
              <path
                d="M10 8H6C4.89543 8 4 8.89543 4 10V14C4 15.1046 4.89543 16 6 16H8C9.10457 16 10 15.1046 10 14V8ZM10 8C10 5.79086 8.20914 4 6 4"
                stroke="url(#quote-grad)"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
              <path
                d="M20 8H16C14.8954 8 14 8.89543 14 10V14C14 15.1046 14.8954 16 16 16H18C19.1046 16 20 15.1046 20 14V8ZM20 8C20 5.79086 18.2091 4 16 4"
                stroke="url(#quote-grad)"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
              <defs>
                <linearGradient
                  id="quote-grad"
                  x1="4"
                  y1="10"
                  x2="20"
                  y2="10"
                  gradientUnits="userSpaceOnUse"
                >
                  <stop stopColor="#A45CFF" />
                  <stop offset="1" stopColor="#5CFFF9" />
                </linearGradient>
              </defs>
            </svg>

            {/* Quote */}
            <p className="text-lg leading-relaxed italic mb-6">
              &ldquo;{testimonial.quote}&rdquo;
            </p>

            {/* Author */}
            <div className="flex items-center gap-4">
              <div className="w-12 h-12 btn-primary-gradient rounded-full flex items-center justify-center text-lg font-semibold">
                {testimonial.initial}
              </div>
              <div>
                <p className="font-semibold">{testimonial.author}</p>
                <p className="text-sm text-muted-foreground">{testimonial.tagline}</p>
              </div>
            </div>
          </GlassCard>
        ))}
      </div>
    </LandingSection>
  );
}
