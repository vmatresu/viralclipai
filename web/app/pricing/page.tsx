export default function PricingPage() {
  return (
    <div className="space-y-8">
      <section className="space-y-3">
        <h1 className="text-3xl font-extrabold text-white">Pricing</h1>
        <p className="text-gray-300 max-w-2xl">
          Start free, then upgrade when you are ready to produce clips at scale.
          Plans are per-user and can be changed at any time.
        </p>
      </section>
      <section className="grid md:grid-cols-3 gap-6">
        <div className="glass rounded-2xl p-6 flex flex-col">
          <h2 className="text-xl font-bold text-white mb-2">Free</h2>
          <p className="text-3xl font-extrabold text-white mb-4">$0</p>
          <ul className="text-sm text-gray-300 space-y-2 flex-1">
            <li>• Up to 20 clips / month</li>
            <li>• All AI highlight detection features</li>
            <li>• Basic email support</li>
          </ul>
          <div className="mt-4">
            <span className="inline-block px-3 py-1 rounded-full text-xs bg-gray-800 text-gray-300">
              Best for testing
            </span>
          </div>
        </div>
        <div className="glass rounded-2xl p-6 border border-blue-500 flex flex-col">
          <h2 className="text-xl font-bold text-white mb-2">Pro</h2>
          <p className="text-3xl font-extrabold text-white mb-1">$29</p>
          <p className="text-xs text-gray-400 mb-4">per month, per creator</p>
          <ul className="text-sm text-gray-300 space-y-2 flex-1">
            <li>• Up to 500 clips / month</li>
            <li>• Priority processing in the queue</li>
            <li>• TikTok publish integration</li>
            <li>• Email support</li>
          </ul>
          <div className="mt-4">
            <span className="inline-block px-3 py-1 rounded-full text-xs bg-blue-600 text-white">
              Recommended
            </span>
          </div>
        </div>
        <div className="glass rounded-2xl p-6 flex flex-col">
          <h2 className="text-xl font-bold text-white mb-2">Studio</h2>
          <p className="text-3xl font-extrabold text-white mb-1">Contact</p>
          <p className="text-xs text-gray-400 mb-4">for custom pricing</p>
          <ul className="text-sm text-gray-300 space-y-2 flex-1">
            <li>• Higher clip limits & dedicated capacity</li>
            <li>• Team accounts & shared settings</li>
            <li>• Custom integrations & SLAs</li>
          </ul>
          <div className="mt-4">
            <a
              href="/contact"
              className="inline-flex items-center px-4 py-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-sm text-gray-100 font-semibold"
            >
              Talk to us
            </a>
          </div>
        </div>
      </section>
    </div>
  );
}
