export default function AboutPage() {
  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h1 className="text-3xl font-extrabold text-foreground">About Viral Clip AI</h1>
        <p className="text-muted-foreground max-w-2xl">
          Viral Clip AI was built for creators who record long-form commentary,
          reaction, and educational videos but want to stay active on short-form
          platforms without editing all day.
        </p>
      </section>
      <section className="space-y-3">
        <h2 className="text-xl font-semibold text-foreground">What we do</h2>
        <p className="text-muted-foreground max-w-2xl">
          We use state-of-the-art language models to understand your content, find the
          most compelling segments, and export them as social-ready clips with suggested
          titles and descriptions.
        </p>
      </section>
      <section className="space-y-3">
        <h2 className="text-xl font-semibold text-foreground">Product principles</h2>
        <ul className="list-disc list-inside text-muted-foreground space-y-1">
          <li>Respect your time – minimal setup, fast results.</li>
          <li>
            Respect your audience – clips are optimized for retention, not clickbait
            only.
          </li>
          <li>
            Respect your data – per-user isolation with Firebase Auth, Firestore, and
            S3.
          </li>
        </ul>
      </section>
    </div>
  );
}
