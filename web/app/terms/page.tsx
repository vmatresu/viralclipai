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

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">10. Feedback</h2>
            <p className="text-muted-foreground">
              If you provide suggestions, ideas, or feedback, you grant us the right to
              use it without restriction or compensation.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              11. Disclaimer of warranties
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
              12. Limitation of liability
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
              13. Indemnification
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
              14. Copyright complaints
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
                href="mailto:support@viralvideoai.io"
              >
                support@viralvideoai.io
              </a>
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">
              15. Suspension and termination
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
              16. Governing law and dispute resolution
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
              17. Changes to these Terms
            </h2>
            <p className="text-muted-foreground">
              We may update these Terms from time to time. The Last updated date
              indicates when changes were made. Your continued use of the Service after
              changes become effective constitutes acceptance of the updated Terms.
            </p>
          </div>

          <div className="space-y-3">
            <h2 className="text-xl font-semibold text-foreground">18. Contact</h2>
            <p className="text-muted-foreground">
              For questions about these Terms, contact us via our{" "}
              <Link href="/contact" className="underline text-primary">
                Contact page
              </Link>{" "}
              or email{" "}
              <a
                className="underline text-primary"
                href="mailto:support@viralvideoai.io"
              >
                support@viralvideoai.io
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
