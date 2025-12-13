import { Layout, Link2, Wand2 } from "lucide-react";

export function HowItWorks() {
  const steps = [
    {
      icon: <Link2 className="w-8 h-8 text-white" />,
      title: "1. Paste YouTube URL",
      description:
        "Copy any YouTube video link (podcasts, interviews, commentary) and paste it into the processor.",
      color: "bg-blue-500",
    },
    {
      icon: <Layout className="w-8 h-8 text-white" />,
      title: "2. Choose Scenes & Layout",
      description:
        "Select Split View for reaction videos or Full View for immersive content.",
      color: "bg-indigo-500",
    },
    {
      icon: <Wand2 className="w-8 h-8 text-white" />,
      title: "3. AI Magic",
      description:
        "Our AI detects faces, tracks active speakers, and generates vertical clips instantly.",
      color: "bg-purple-500",
    },
  ];

  return (
    <section className="py-24 relative border-y border-white/5 bg-white/[0.02]">
      <div className="container px-4 md:px-6">
        <div className="text-center max-w-2xl mx-auto mb-16 space-y-4">
          <h2 className="h2 text-4xl border-none">How it works</h2>
          <p className="text-muted-foreground text-lg">
            Create weeks worth of content in just a few minutes.
          </p>
        </div>

        <div className="grid md:grid-cols-3 gap-8 relative">
          {/* Connector Line (Desktop) */}
          <div className="hidden md:block absolute top-[2.5rem] left-[16%] right-[16%] h-0.5 bg-gradient-to-r from-blue-500/50 via-indigo-500/50 to-purple-500/50" />

          {steps.map((step, i) => (
            <div
              key={i}
              className="relative flex flex-col items-center text-center space-y-6 group"
            >
              <div
                className={`
                relative z-10 w-20 h-20 rounded-2xl flex items-center justify-center 
                shadow-lg transition-transform duration-300 group-hover:scale-110 group-hover:rotate-3
                ${step.color}
              `}
              >
                {step.icon}
                <div
                  className={`absolute inset-0 rounded-2xl blur-xl opacity-40 ${step.color}`}
                />
              </div>

              <div className="space-y-2">
                <h3 className="text-xl font-bold">{step.title}</h3>
                <p className="text-muted-foreground leading-relaxed">
                  {step.description}
                </p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
