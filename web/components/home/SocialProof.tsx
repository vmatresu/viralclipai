export function SocialProof() {
  return (
    <section className="py-12 border-y border-white/5 bg-black/20">
      <div className="container px-4 text-center">
        <p className="text-sm font-medium text-muted-foreground uppercase tracking-widest mb-8">
          Trusted by creators from
        </p>
        <div className="flex flex-wrap justify-center items-center gap-12 md:gap-20 grayscale opacity-50">
          {/* Placeholder Logos - Text for now as we don't have SVGs */}
          <span className="text-xl font-bold font-serif">YouTubers</span>
          <span className="text-xl font-bold font-sans tracking-tighter">
            PODCASTERS
          </span>
          <span className="text-xl font-bold italic">Streamers</span>
          <span className="text-xl font-bold font-mono">AGENCIES</span>
        </div>
      </div>
    </section>
  );
}
