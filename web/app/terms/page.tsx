import Link from "next/link";

import { PageWrapper } from "@/components/landing/PageWrapper";

export default function TermsPage() {
  return (
    <PageWrapper>
      <div className="max-w-4xl mx-auto space-y-8">
        <header className="space-y-3">
          <h1 className="text-3xl md:text-4xl font-extrabold text-foreground">
            Terms of Service
          </h1>
          <p className="text-sm text-muted-foreground">
            <span className="font-semibold text-foreground">Last updated:</span>{" "}
            December 17, 2025
          </p>
          <p className="text-muted-foreground">
            These Terms of Service (the Terms) govern your access to and use of
            ViralClipAI.io (the Site) and the Viral Clip AI services, applications, and
            related features (collectively, the Service).
          </p>
          <p className="text-muted-foreground">
            The Service is owned and operated by{" "}
            <span className="font-semibold text-foreground">
              MATRESU.COM Corporation
            </span>{" "}
            (MATRESU, we, us, or our).
          </p>
        </header>

        <section className="glass rounded-2xl p-6 md:p-10 space-y-6">
          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              1. Acceptance of these Terms
            </h2>
            <p className="text-muted-foreground">
              By accessing or using the Service, you agree to be bound by these Terms.
              If you do not agree, do not use the Service.
            </p>
            <p className="text-muted-foreground">
              If you use the Service on behalf of an organization, you represent that
              you have authority to bind that organization to these Terms, and you
              includes that organization.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">2. Eligibility</h2>
            <p className="text-muted-foreground">
              You must be at least 13 years old (or the minimum age required in your
              jurisdiction) to use the Service. If you are under 18, you may use the
              Service only with the involvement and consent of a parent or legal
              guardian.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              3. Accounts and authentication
            </h2>
            <p className="text-muted-foreground">
              You may need an account to access parts of the Service. You are
              responsible for maintaining the confidentiality of your account
              credentials and for all activity that occurs under your account.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">4. The Service</h2>
            <p className="text-muted-foreground">
              Viral Clip AI helps you transform longer videos into shorter clips and
              related assets (for example, titles, descriptions, thumbnails, and
              metadata). The Service may use automated systems and AI models to analyze
              content and generate outputs.
            </p>
            <p className="text-muted-foreground">
              The Service may change over time. We may add, remove, or modify features,
              and we may suspend or discontinue the Service (in whole or in part) at any
              time.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              5. Your content and permissions
            </h2>
            <p className="text-muted-foreground">
              User Content means any content you submit to the Service or cause the
              Service to access, including video URLs, uploaded files, prompts,
              descriptions, and related materials.
            </p>
            <p className="text-muted-foreground">You represent and warrant that:</p>
            <ul className="list-disc list-inside space-y-2 text-muted-foreground">
              <li>
                You own or have the necessary rights, permissions, and consents to
                provide User Content and to authorize us to process it.
              </li>
              <li>
                Your User Content and your use of the Service do not violate any laws or
                infringe any third-party rights (including copyright, privacy,
                publicity, or contractual rights).
              </li>
              <li>
                If you submit a video URL (for example, a YouTube URL), you are
                responsible for complying with the applicable platform terms and
                policies.
              </li>
            </ul>
            <p className="text-muted-foreground">
              You retain ownership of your User Content. However, you grant MATRESU a
              non-exclusive, worldwide, royalty-free license to host, store, cache,
              reproduce, transcode, analyze, process, modify (as needed for formatting),
              and display your User Content solely to provide, maintain, and improve the
              Service and to produce the outputs you request.
            </p>
            <p className="text-muted-foreground">
              If you choose to create share links or otherwise make outputs available to
              others through the Service, you authorize us to deliver those outputs to
              anyone with the applicable link or access permissions.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">6. Acceptable use</h2>
            <p className="text-muted-foreground">You agree not to:</p>
            <ul className="list-disc list-inside space-y-2 text-muted-foreground">
              <li>
                Use the Service for unlawful, harmful, or abusive purposes, including
                harassment, threats, or exploitation.
              </li>
              <li>
                Submit or process content that infringes intellectual property rights or
                violates another person’s privacy or publicity rights.
              </li>
              <li>
                Attempt to reverse engineer, scrape, probe, or disrupt the Service,
                including by bypassing security controls, rate limits, or access
                restrictions.
              </li>
              <li>
                Upload or transmit malware or other code intended to compromise systems
                or data.
              </li>
              <li>
                Use the Service to generate or distribute content that is illegal,
                deceptive, or that you do not have rights to distribute.
              </li>
            </ul>
            <p className="text-muted-foreground">
              We may suspend or terminate access if we reasonably believe you have
              violated these Terms.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              7. Intellectual property
            </h2>
            <p className="text-muted-foreground">
              The Service, including its software, design, and trademarks, is owned by
              MATRESU and its licensors and is protected by intellectual property laws.
              Except for the limited rights expressly granted to you, no rights are
              granted.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              8. Third-party services
            </h2>
            <p className="text-muted-foreground">
              The Service may interact with or depend on third-party services (for
              example, video platforms, cloud providers, and analytics). Your use of
              those third-party services may be subject to their own terms and privacy
              policies. MATRESU is not responsible for third-party services.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              9. Fees, plans, and billing
            </h2>
            <p className="text-muted-foreground">
              Certain features may be offered under paid plans or usage limits.
              Availability, limits, and pricing may change from time to time. Unless
              otherwise required by law or explicitly stated in writing by MATRESU,
              payments are non-refundable.
            </p>
          </div>

          <div className="space-y-6">
            <h2 className="text-xl font-semibold text-foreground">
              10. Credits & Subscription Plans
            </h2>
            <p className="text-muted-foreground">
              Viral Clip AI uses a credit-based system for processing and rendering
              video clips. Credits are consumed when you analyze videos, render clips
              with different styles, and use optional add-on features. Below is an
              overview of our credit costs and plan allowances.
            </p>

            {/* Credit Costs Table */}
            <div className="space-y-3">
              <h3 className="text-lg font-medium text-foreground">Credit Costs</h3>
              <p className="text-sm text-muted-foreground">
                Each action consumes a specified number of credits from your monthly
                allowance.
              </p>
              <div className="overflow-x-auto">
                <table className="w-full text-sm border-collapse">
                  <thead>
                    <tr className="border-b border-border">
                      <th className="text-left py-3 px-4 font-semibold text-foreground">
                        Feature
                      </th>
                      <th className="text-right py-3 px-4 font-semibold text-foreground">
                        Credits
                      </th>
                    </tr>
                  </thead>
                  <tbody className="text-muted-foreground">
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">Video Analysis</td>
                      <td className="py-3 px-4 text-right font-medium">3</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">
                        Original export (no cropping)
                      </td>
                      <td className="py-3 px-4 text-right font-medium">5 per clip</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">
                        Static styles (Split, Focus crops, Split Fast)
                      </td>
                      <td className="py-3 px-4 text-right font-medium">10 per clip</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">
                        Motion styles (Motion, Motion Split)
                      </td>
                      <td className="py-3 px-4 text-right font-medium">10 per clip</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">
                        Smart Face styles (Smart Face, Smart Face Split)
                      </td>
                      <td className="py-3 px-4 text-right font-medium">20 per clip</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">
                        Active Speaker styles (Active Speaker, Active Speaker Split)
                      </td>
                      <td className="py-3 px-4 text-right font-medium">20 per clip</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">Streamer / Streamer Split styles</td>
                      <td className="py-3 px-4 text-right font-medium">10 per clip</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4">Cinematic styles (Premium AI)</td>
                      <td className="py-3 px-4 text-right font-medium">30 per clip</td>
                    </tr>
                    <tr className="border-b border-border/50 bg-muted/30">
                      <td className="py-3 px-4 italic">
                        Add-on: Scene originals download
                      </td>
                      <td className="py-3 px-4 text-right font-medium">+5 per scene</td>
                    </tr>
                    <tr className="border-b border-border/50 bg-muted/30">
                      <td className="py-3 px-4 italic">
                        Add-on: Cut silent parts (VAD)
                      </td>
                      <td className="py-3 px-4 text-right font-medium">+5 per scene</td>
                    </tr>
                    <tr className="bg-muted/30">
                      <td className="py-3 px-4 italic">Add-on: Object detection</td>
                      <td className="py-3 px-4 text-right font-medium">+10</td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>

            {/* Monthly Limits Table */}
            <div className="space-y-3">
              <h3 className="text-lg font-medium text-foreground">
                Monthly Limits by Plan
              </h3>
              <p className="text-sm text-muted-foreground">
                Each subscription plan includes a monthly credit allowance and cloud
                storage allocation. Credits reset at the start of each billing cycle;
                unused credits do not roll over.
              </p>
              <div className="overflow-x-auto">
                <table className="w-full text-sm border-collapse">
                  <thead>
                    <tr className="border-b border-border">
                      <th className="text-left py-3 px-4 font-semibold text-foreground">
                        Plan
                      </th>
                      <th className="text-right py-3 px-4 font-semibold text-foreground">
                        Monthly Credits
                      </th>
                      <th className="text-right py-3 px-4 font-semibold text-foreground">
                        Storage
                      </th>
                    </tr>
                  </thead>
                  <tbody className="text-muted-foreground">
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4 font-medium text-foreground">Free</td>
                      <td className="py-3 px-4 text-right">200</td>
                      <td className="py-3 px-4 text-right">1 GB</td>
                    </tr>
                    <tr className="border-b border-border/50">
                      <td className="py-3 px-4 font-medium text-foreground">Pro</td>
                      <td className="py-3 px-4 text-right">4,000</td>
                      <td className="py-3 px-4 text-right">30 GB</td>
                    </tr>
                    <tr>
                      <td className="py-3 px-4 font-medium text-foreground">Studio</td>
                      <td className="py-3 px-4 text-right">12,000</td>
                      <td className="py-3 px-4 text-right">150 GB</td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>

            <p className="text-sm text-muted-foreground">
              For current pricing and to manage your subscription, please visit our{" "}
              <Link href="/pricing" className="underline text-primary">
                Pricing page
              </Link>
              . We reserve the right to adjust credit costs and plan limits with
              reasonable notice.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">11. Feedback</h2>
            <p className="text-muted-foreground">
              If you provide suggestions, ideas, or feedback, you grant us the right to
              use it without restriction or compensation.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              12. Disclaimer of warranties
            </h2>
            <p className="text-muted-foreground">
              THE SERVICE IS PROVIDED AS IS AND AS AVAILABLE. TO THE MAXIMUM EXTENT
              PERMITTED BY LAW, MATRESU DISCLAIMS ALL WARRANTIES, EXPRESS OR IMPLIED,
              INCLUDING WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE,
              AND NON-INFRINGEMENT. WE DO NOT WARRANT THAT THE SERVICE WILL BE
              UNINTERRUPTED, ERROR-FREE, OR SECURE.
            </p>
            <p className="text-muted-foreground">
              AI-generated outputs may be inaccurate, incomplete, or unsuitable for your
              purposes. You are responsible for reviewing outputs before publishing or
              relying on them.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              13. Limitation of liability
            </h2>
            <p className="text-muted-foreground">
              TO THE MAXIMUM EXTENT PERMITTED BY LAW, IN NO EVENT WILL MATRESU OR ITS
              AFFILIATES, OFFICERS, EMPLOYEES, AGENTS, OR LICENSORS BE LIABLE FOR ANY
              INDIRECT, INCIDENTAL, SPECIAL, CONSEQUENTIAL, EXEMPLARY, OR PUNITIVE
              DAMAGES, OR FOR ANY LOSS OF PROFITS, REVENUE, DATA, OR GOODWILL, ARISING
              OUT OF OR RELATED TO YOUR USE OF (OR INABILITY TO USE) THE SERVICE.
            </p>
            <p className="text-muted-foreground">
              TO THE MAXIMUM EXTENT PERMITTED BY LAW, MATRESU’S TOTAL LIABILITY FOR ALL
              CLAIMS ARISING OUT OF OR RELATED TO THE SERVICE WILL NOT EXCEED THE AMOUNT
              YOU PAID TO MATRESU FOR THE SERVICE IN THE TWELVE (12) MONTHS IMMEDIATELY
              PRECEDING THE EVENT GIVING RISE TO THE CLAIM, OR US$100 IF YOU HAVE NOT
              PAID ANY AMOUNTS.
            </p>
            <p className="text-muted-foreground">
              Some jurisdictions do not allow certain limitations of liability, so some
              of the above limitations may not apply to you.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              14. Indemnification
            </h2>
            <p className="text-muted-foreground">
              You agree to defend, indemnify, and hold harmless MATRESU and its
              affiliates, officers, employees, and agents from and against any claims,
              liabilities, damages, losses, and expenses (including reasonable
              attorneys’ fees) arising out of or related to your User Content or your
              use of the Service, including any alleged infringement or violation of
              third-party rights.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              15. Copyright complaints
            </h2>
            <p className="text-muted-foreground">
              If you believe content on the Service infringes your copyright, please
              contact us with sufficient information to identify the work and the
              allegedly infringing material, and your contact information.
            </p>
            <p className="text-muted-foreground">
              Contact:{" "}
              <a
                className="underline text-primary"
                href="mailto:contact@viralclipai.io"
              >
                contact@viralclipai.io
              </a>
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              16. Suspension and termination
            </h2>
            <p className="text-muted-foreground">
              We may suspend or terminate your access to the Service at any time if we
              reasonably believe you have violated these Terms, if required by law, or
              to protect the Service or other users.
            </p>
            <p className="text-muted-foreground">
              You may stop using the Service at any time. Certain provisions of these
              Terms will survive termination, including intellectual property,
              disclaimers, limitation of liability, indemnification, and dispute
              resolution.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              17. Governing law and dispute resolution
            </h2>
            <p className="text-muted-foreground">
              These Terms are governed by the laws of the jurisdiction where MATRESU is
              established, without regard to conflict of law principles. Venue and
              dispute resolution procedures may depend on your location and applicable
              law.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              18. Changes to these Terms
            </h2>
            <p className="text-muted-foreground">
              We may update these Terms from time to time. The Last updated date
              indicates when changes were made. Your continued use of the Service after
              changes become effective constitutes acceptance of the updated Terms.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">19. Contact</h2>
            <p className="text-muted-foreground">
              For questions about these Terms, contact us via our{" "}
              <Link href="/contact" className="underline text-primary">
                Contact page
              </Link>{" "}
              or email{" "}
              <a
                className="underline text-primary"
                href="mailto:contact@viralclipai.io"
              >
                contact@viralclipai.io
              </a>
              .
            </p>
            <p className="text-muted-foreground">
              For information about how we handle personal data, please review our{" "}
              <Link href="/privacy" className="underline text-primary">
                Privacy Policy
              </Link>
              .
            </p>
          </div>
        </section>
      </div>
    </PageWrapper>
  );
}
