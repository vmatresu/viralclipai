import { Clock, History, Smartphone, Sparkles, Users, Zap } from "lucide-react";

export function FeatureHighlights() {
  const features = [
    {
      icon: <Sparkles className="w-6 h-6 text-amber-400" />,
      title: "AI Highlight Detection",
      description:
        "Automatically identifies the most engaging, viral-worthy segments from long videos.",
    },
    {
      icon: <Smartphone className="w-6 h-6 text-blue-400" />,
      title: "Vertical-First Formats",
      description:
        "Optimized for TikTok, Reels, and Shorts with intelligent Split View and Full View layouts.",
    },
    {
      icon: <Users className="w-6 h-6 text-green-400" />,
      title: "Active Speaker Tracking",
      description:
        "AI camera follows the speaker's face and movement, just like a human editor.",
    },
    {
      icon: <History className="w-6 h-6 text-purple-400" />,
      title: "Workspace History",
      description: "All your clips are saved securely. Re-download or re-edit anytime.",
    },
    {
      icon: <Zap className="w-6 h-6 text-red-400" />,
      title: "Lightning Fast",
      description:
        "Process hour-long podcasts in minutes with our distributed GPU cloud.",
    },
    {
      icon: <Clock className="w-6 h-6 text-teal-400" />,
      title: "Auto-Captions (Beta)",
      description:
        "Generate accurate subtitles automatically to boost engagement and retention.",
    },
  ];

  return (
    <section className="py-24 container px-4 md:px-6">
      <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-6">
        {features.map((f) => (
          <div
            key={f.title}
            className="group p-6 rounded-2xl border border-brand-100 bg-white shadow-sm hover:-translate-y-0.5 hover:shadow-md transition-all dark:border-white/5 dark:bg-white/5"
          >
            <div className="w-12 h-12 rounded-lg bg-brand-50 text-brand-700 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform dark:bg-white/5 dark:text-white">
              {f.icon}
            </div>
            <h3 className="text-xl font-bold mb-2">{f.title}</h3>
            <p className="text-muted-foreground">{f.description}</p>
          </div>
        ))}
      </div>
    </section>
  );
}
