import Link from "next/link";

import { PageWrapper } from "@/components/landing/PageWrapper";

export default function PrivacyPage() {
  return (
    <PageWrapper>
      <div className="max-w-4xl mx-auto space-y-8">
        <header className="space-y-3">
          <h1 className="text-3xl md:text-4xl font-extrabold text-foreground">
            Privacy Policy
          </h1>
          <p className="text-sm text-muted-foreground">
            <span className="font-semibold text-foreground">Last updated:</span>{" "}
            December 17, 2025
          </p>
          <p className="text-muted-foreground">
            This Privacy Policy explains how{" "}
            <span className="font-semibold text-foreground">
              MATRESU.COM Corporation
            </span>
            (MATRESU, we, us, or our) collects, uses, and shares information when you
            use ViralClipAI.io (the Site) and the Viral Clip AI services (collectively,
            the Service).
          </p>
        </header>

        <section className="glass rounded-2xl p-6 md:p-10 space-y-6">
          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              1. Information we collect
            </h2>
            <p className="text-muted-foreground">
              The information we collect depends on how you use the Service.
            </p>

            <div className="space-y-2">
              <h3 className="text-lg font-semibold text-foreground">
                1.1 Account information
              </h3>
              <p className="text-muted-foreground">
                When you sign in, we receive account information such as your email
                address and an account identifier from our authentication provider.
              </p>
            </div>

            <div className="space-y-2">
              <h3 className="text-lg font-semibold text-foreground">
                1.2 Content you provide and content we process
              </h3>
              <p className="text-muted-foreground">
                The Service processes content you submit or make available to us,
                including:
              </p>
              <ul className="list-disc list-inside space-y-2 text-muted-foreground">
                <li>
                  Video URLs or identifiers you submit (for example, YouTube URLs)
                </li>
                <li>Uploaded media (if you upload files)</li>
                <li>Prompts, titles, descriptions, and other text inputs</li>
                <li>Generated outputs such as clips, thumbnails, and metadata</li>
              </ul>
              <p className="text-muted-foreground">
                You should not submit content you do not have the right to use or
                distribute.
              </p>
            </div>

            <div className="space-y-2">
              <h3 className="text-lg font-semibold text-foreground">
                1.3 Usage and device information
              </h3>
              <p className="text-muted-foreground">
                We may collect information about how you use the Service, including log
                information (such as IP address, timestamps, pages viewed, interactions,
                and error logs) and device information (such as browser and operating
                system).
              </p>
            </div>

            <div className="space-y-2">
              <h3 className="text-lg font-semibold text-foreground">
                1.4 Cookies and similar technologies
              </h3>
              <p className="text-muted-foreground">
                We use cookies and similar technologies (such as local storage) to
                provide the Service, keep you signed in, remember preferences, and help
                us understand usage.
              </p>
              <p className="text-muted-foreground">
                If your browser has Do Not Track enabled, we attempt to respect that
                signal for analytics collection where feasible.
              </p>
            </div>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              2. How we use information
            </h2>
            <p className="text-muted-foreground">We use information to:</p>
            <ul className="list-disc list-inside space-y-2 text-muted-foreground">
              <li>Provide, operate, and maintain the Service</li>
              <li>Process videos and generate the outputs you request</li>
              <li>Secure the Service, prevent abuse, and enforce our terms</li>
              <li>Improve features, performance, and user experience</li>
              <li>Communicate with you about support and product updates</li>
              <li>Comply with legal obligations</li>
            </ul>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              3. How we share information
            </h2>
            <p className="text-muted-foreground">
              We may share information in the following situations:
            </p>
            <ul className="list-disc list-inside space-y-2 text-muted-foreground">
              <li>
                <span className="font-semibold text-foreground">
                  Service providers:
                </span>{" "}
                We use third-party providers to help run the Service (for example, cloud
                hosting and storage, analytics, authentication, and AI or media
                processing services). These providers process information on our behalf
                under contractual obligations.
              </li>
              <li>
                <span className="font-semibold text-foreground">
                  Sharing you enable:
                </span>{" "}
                If you create share links or otherwise configure content sharing,
                content and outputs may be accessible to anyone with the applicable link
                or permissions.
              </li>
              <li>
                <span className="font-semibold text-foreground">Legal and safety:</span>{" "}
                We may disclose information if required by law or if we believe it is
                necessary to protect the rights, safety, and security of MATRESU, our
                users, or others.
              </li>
              <li>
                <span className="font-semibold text-foreground">
                  Business transfers:
                </span>{" "}
                If we are involved in a merger, acquisition, financing, or sale of
                assets, information may be transferred as part of that transaction.
              </li>
            </ul>
            <p className="text-muted-foreground">
              We do not sell personal information in the traditional sense.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">4. Data retention</h2>
            <p className="text-muted-foreground">
              We retain information for as long as necessary to provide the Service,
              comply with legal obligations, resolve disputes, and enforce agreements.
            </p>
            <p className="text-muted-foreground">
              Working copies of source media may be used as temporary files during
              processing and then removed. Clips and related outputs may be stored until
              you delete them or request deletion.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">5. Security</h2>
            <p className="text-muted-foreground">
              We use reasonable administrative, technical, and organizational measures
              designed to protect information. However, no method of transmission or
              storage is completely secure.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              6. Your choices and rights
            </h2>
            <p className="text-muted-foreground">
              Depending on your location, you may have rights to access, correct,
              delete, or restrict the processing of your personal information.
            </p>
            <p className="text-muted-foreground">You can also:</p>
            <ul className="list-disc list-inside space-y-2 text-muted-foreground">
              <li>
                Delete clips and related outputs from your account where available
              </li>
              <li>Request account deletion through our contact channels</li>
              <li>
                Opt out of analytics by setting analytics consent to disabled in your
                browser (where available) and by enabling Do Not Track
              </li>
            </ul>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              7. International data transfers
            </h2>
            <p className="text-muted-foreground">
              The Service may be provided using infrastructure and service providers
              located in different countries. As a result, information may be
              transferred to and processed in countries other than your own.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">8. Children</h2>
            <p className="text-muted-foreground">
              The Service is not directed to children under 13. We do not knowingly
              collect personal information from children under 13.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">
              9. Changes to this Privacy Policy
            </h2>
            <p className="text-muted-foreground">
              We may update this Privacy Policy from time to time. The Last updated date
              indicates when changes were made. Your continued use of the Service after
              changes become effective constitutes acceptance of the updated Privacy
              Policy.
            </p>
          </div>

          <div className="space-y-2">
            <h2 className="text-xl font-semibold text-foreground">10. Contact</h2>
            <p className="text-muted-foreground">
              For questions or requests related to privacy, contact us via our{" "}
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
              For security disclosures, see the contact information in{" "}
              <Link href="/.well-known/security.txt" className="underline text-primary">
                /.well-known/security.txt
              </Link>
              .
            </p>
          </div>
        </section>
      </div>
    </PageWrapper>
  );
}
