export default function ContactPage() {
  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h1 className="text-3xl font-extrabold text-foreground">Contact us</h1>
        <p className="text-muted-foreground max-w-2xl">
          Questions about pricing, onboarding your team, or feature requests? Reach out
          and we&apos;ll get back to you.
        </p>
      </section>
      <div className="grid gap-6 md:grid-cols-2">
        <section className="glass rounded-2xl p-6 space-y-3">
          <h2 className="text-xl font-semibold text-foreground">Email</h2>
          <p className="text-muted-foreground">
            You can contact us at:
            <br />
            <a
              href="mailto:support@viralvideoai.io"
              className="text-brand-600 font-semibold underline hover:text-brand-700"
            >
              support@viralvideoai.io
            </a>
          </p>
        </section>
        <section className="glass rounded-2xl p-6 space-y-3">
          <h2 className="text-xl font-semibold text-foreground">Response times</h2>
          <p className="text-muted-foreground">
            We aim to respond within 1 business day for Free users and within a few
            business hours for Pro and Studio customers.
          </p>
        </section>
      </div>
    </div>
  );
}
