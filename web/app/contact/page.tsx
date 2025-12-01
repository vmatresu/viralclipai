export default function ContactPage() {
  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h1 className="text-3xl font-extrabold text-white">Contact us</h1>
        <p className="text-gray-300 max-w-2xl">
          Questions about pricing, onboarding your team, or feature requests? Reach out
          and we&apos;ll get back to you.
        </p>
      </section>
      <section className="space-y-3">
        <h2 className="text-xl font-semibold text-white">Email</h2>
        <p className="text-gray-300">
          You can contact us at:
          <br />
          <a
            href="mailto:support@viralvideoai.io"
            className="text-blue-400 hover:text-blue-300 underline"
          >
            support@viralvideoai.io
          </a>
        </p>
      </section>
      <section className="space-y-3">
        <h2 className="text-xl font-semibold text-white">Response times</h2>
        <p className="text-gray-300">
          We aim to respond within 1 business day for Free users and within a few
          business hours for Pro and Studio customers.
        </p>
      </section>
    </div>
  );
}
