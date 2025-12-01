export default function DocsPage() {
  return (
    <div className="space-y-8">
      <section className="space-y-3">
        <h1 className="text-3xl font-extrabold text-white">Documentation</h1>
        <p className="text-gray-300 max-w-2xl">
          Learn how Viral Clip AI works, how to get started, and how we handle
          security and limits.
        </p>
      </section>

      <section className="space-y-2">
        <h2 className="text-xl font-semibold text-white">Getting started</h2>
        <ol className="list-decimal list-inside text-gray-300 space-y-1">
          <li>Sign in using your Google account (Firebase Auth).</li>
          <li>
            Paste a YouTube URL on the Home page and choose an output style.
          </li>
          <li>Wait for the AI analysis and clipping to finish.</li>
          <li>Download clips or publish directly to TikTok (if configured).</li>
        </ol>
      </section>

      <section className="space-y-2">
        <h2 className="text-xl font-semibold text-white">Authentication</h2>
        <p className="text-gray-300 max-w-2xl">
          We use Firebase Auth on the frontend. The browser obtains a
          short-lived ID token which is sent to the FastAPI backend via
          WebSockets and HTTP headers. The backend verifies the token with
          Firebase Admin and isolates data by user ID.
        </p>
      </section>

      <section className="space-y-2">
        <h2 className="text-xl font-semibold text-white">Limits and plans</h2>
        <p className="text-gray-300 max-w-2xl">
          Each user has a plan (Free or Pro) that controls how many clips can be
          generated per calendar month. The backend checks your monthly usage
          before starting a new job and returns an error if you are over quota.
          See the{" "}
          <a
            href="/pricing"
            className="text-blue-400 hover:text-blue-300 underline"
          >
            Pricing
          </a>{" "}
          page for details.
        </p>
      </section>

      <section className="space-y-2">
        <h2 className="text-xl font-semibold text-white">Storage & security</h2>
        <ul className="list-disc list-inside text-gray-300 space-y-1 max-w-2xl">
          <li>
            S3 is used for storing clips, thumbnails, and analysis metadata.
          </li>
          <li>Each user&apos;s assets are stored under a per-user prefix.</li>
          <li>Downloads use short-lived presigned URLs.</li>
          <li>
            FireStore tracks usage, history, and user settings (including TikTok
            tokens).
          </li>
        </ul>
      </section>

      <section className="space-y-2">
        <h2 className="text-xl font-semibold text-white">FAQ</h2>
        <div className="space-y-3 text-gray-300 max-w-2xl">
          <div>
            <h3 className="font-semibold text-white">
              Which videos work best?
            </h3>
            <p>
              Commentary, reaction, educational, and podcast-style videos where
              you speak throughout the video usually produce the strongest
              highlights.
            </p>
          </div>
          <div>
            <h3 className="font-semibold text-white">
              Can I use non-YouTube sources?
            </h3>
            <p>
              Today we focus on YouTube URLs. If your source can be imported
              into YouTube (public or unlisted), it will likely work well.
            </p>
          </div>
          <div>
            <h3 className="font-semibold text-white">
              Do you keep my videos forever?
            </h3>
            <p>
              Raw source videos are used as temporary working files and then
              removed. Clips and thumbnails are stored in S3 under your account
              until you delete them or request deletion.
            </p>
          </div>
          <div>
            <h3 className="font-semibold text-white">
              How do I upgrade or cancel?
            </h3>
            <p>
              Plan management is tied to your account. Today upgrades are
              managed manually—contact us via{" "}
              <a
                href="/contact"
                className="text-blue-400 hover:text-blue-300 underline"
              >
                Contact
              </a>{" "}
              and we&apos;ll help.
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}
