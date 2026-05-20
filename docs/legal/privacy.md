# Privacy Policy

_Last updated: 2026-05-20_

This Privacy Policy explains what personal information is involved when you use the Entracte website ([entracte.drmowinckels.io](https://entracte.drmowinckels.io)), the Entracte desktop application ("the App"), or the Entracte Supporter Pack, and how it is handled.

Entracte is operated by Athanasia Mowinckel, a sole trader based in Norway ("we", "us", "our"). For questions about this policy or to exercise your data rights, contact [entracte@drmowinckels.io](mailto:entracte@drmowinckels.io). For bug reports and general questions, please use [GitHub issues](https://github.com/drmowinckels/entracte/issues) instead — they're public, so don't include personal data there.

The short version: **the App stores everything locally on your computer and does not send your usage data anywhere.** The website is static and has no analytics. The only personal data we touch is what's needed to process a Supporter Pack purchase, which is handled by Lemon Squeezy as merchant of record, and to validate a Supporter Pack license key when one has been entered.

## 1. The App

The Entracte App runs entirely on your computer. It does not include analytics, telemetry, crash reporting, or user tracking of any kind. The App's source code is open and auditable at [github.com/drmowinckels/entracte](https://github.com/drmowinckels/entracte).

### 1.1 Data stored locally

The App stores the following on your machine, and only on your machine:

- **Settings** — your break schedule, profiles, theme choice, hook commands, and other preferences (`settings.json`, in the OS app-config directory).
- **Break history** — local statistics on breaks taken, skipped, postponed, and suppressed, used to render the stats screen and the 12-week heatmap.
- **Supporter Pack record** — if you have entered a Supporter Pack license key, a small `supporter.json` record in the OS app-config directory, containing the license token and last-validation timestamp.
- **Logs** — a rolling local log file for troubleshooting.

None of this data leaves your machine through normal use of the App. You can clear break history at any time from the Stats screen, and you can remove the Supporter Pack record at any time from **About → Supporter**.

Settings, break history, and logs are stored in the OS app-config directory:

- **macOS** — `~/Library/Application Support/io.drmowinckels.entracte/`
- **Windows** — `%APPDATA%\io.drmowinckels.entracte\`
- **Linux** — `~/.config/io.drmowinckels.entracte/`

### 1.2 OS sensors used for break suppression

To decide whether to interrupt you with a break, the App reads signals from the operating system: whether **Do Not Disturb** is on, whether the **camera** is currently in use (so it can skip breaks during meetings), and **system idle time** (so it doesn't fire a break when you've already stepped away). These signals are read in-process and are never transmitted off your machine.

### 1.3 Update checks

The App may check for new releases by contacting the Entracte GitHub release feed. This request reveals only your IP address to GitHub and contains no personally identifying information from the App. You can disable update checks in Settings.

## 2. Supporter Pack purchases

If you purchase the Entracte Supporter Pack, the following personal data is involved.

### 2.1 Data processed by Lemon Squeezy

Supporter Pack purchases are processed by **Lemon Squeezy** (Lemon Squeezy LLC), who acts as **merchant of record**. Lemon Squeezy collects and processes the information needed to take payment, calculate VAT or sales tax, issue an invoice, and email you a receipt and your license key. This typically includes your name, email address, billing address (for tax purposes), and payment-method details.

Lemon Squeezy is the data controller for that processing. Their handling of your data is governed by the [Lemon Squeezy Privacy Policy](https://www.lemonsqueezy.com/privacy). Payment-method details (card number, CVV, etc.) are handled by Lemon Squeezy and its payment processors; we never see them.

### 2.2 Data we receive from Lemon Squeezy

For each completed purchase, Lemon Squeezy forwards to us:

- your **email address** (so we can support you if you need help with your license),
- the **order ID** and **license key** issued for the purchase,
- the **product**, **price**, **currency**, and **purchase date**,
- the **country** used for tax purposes.

We act as the data controller for this subset of information. We use it solely to:

- support your purchase (e.g., resending a license key, processing a refund),
- validate the license key when your App calls our validation endpoint (see §2.3),
- meet our bookkeeping and tax-record obligations.

We do not sell, rent, or share this information with third parties for marketing purposes.

### 2.3 License validation calls

When you activate a Supporter Pack license key in the App, and once per day thereafter while the key is active, the App calls the Lemon Squeezy License API to check that the key is still valid. Each call sends the **license key** and a **machine identifier** that binds the key to one machine at a time. It does not send your settings, your break history, or any other personal data.

If you are offline, the App falls back to a 30-day grace window before re-checking, so brief connectivity gaps do not lock you out.

If a key is later refunded or revoked, the next validation call returns "invalid" and the App removes the local `supporter.json` record on its own.

### 2.4 Lawful basis (EU/EEA)

For EU/EEA users, our lawful bases under the GDPR are:

- **Performance of a contract** — to process your purchase, deliver the license key, and validate it.
- **Legal obligation** — to retain billing records required by Norwegian tax and accounting law.
- **Legitimate interests** — to provide customer support and prevent abuse of the licensing system.

### 2.5 Retention

We retain purchase records (order ID, email, license key, country, amount, date) for as long as required by Norwegian bookkeeping law (currently five years from the end of the financial year of the transaction), after which they are deleted or anonymised.

You may request a copy of, correction of, or deletion of any data we hold about you that is not subject to the legal-retention requirement above; see §5.

## 3. The website

The Entracte website is a static documentation site served from GitHub Pages and built with [VitePress](https://vitepress.dev). The website itself does not set tracking cookies, run analytics, or fingerprint visitors.

Standard HTTP server logs (IP address, user agent, requested path, timestamp) are recorded transiently by the hosting provider (GitHub) for operational and security purposes. We do not access or process these logs ourselves. See the [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) for details.

The site embeds video clips and screenshots served from the same origin; no third-party embeds, fonts, or scripts run by default.

## 4. Sharing and international transfers

Aside from the use of Lemon Squeezy as merchant of record (see §2.1), we do not share your personal data with third parties. We do not sell personal data.

Because Lemon Squeezy and GitHub are based outside the EU/EEA, processing your purchase or accessing the website may involve transfers of your personal data to the United States or other jurisdictions. Those providers maintain their own safeguards for international transfers, including, where applicable, the EU–US Data Privacy Framework and the European Commission's Standard Contractual Clauses.

## 5. Your rights

Depending on where you live, you may have the right to:

- access the personal data we hold about you,
- correct inaccurate data,
- request deletion of data that is not subject to a legal-retention requirement,
- object to or restrict certain processing,
- receive a portable copy of data you provided to us,
- lodge a complaint with a supervisory authority (in Norway, the [Datatilsynet](https://www.datatilsynet.no)).

To exercise any of these rights, email [entracte@drmowinckels.io](mailto:entracte@drmowinckels.io). We will respond within the timeframes required by applicable law (under the GDPR, within one month).

## 6. Children

Entracte is not directed to children under 13 and we do not knowingly collect personal data from children. If you believe a child has provided personal data through the Supporter Pack checkout, contact us and we will delete it.

## 7. Security

Personal data we hold (purchase records, license keys) is stored on systems protected by access controls and is transmitted over TLS. No system is perfectly secure; if a breach occurs that affects your personal data, we will notify you and the relevant supervisory authority as required by law.

## 8. Changes to this policy

We may update this policy from time to time. The "Last updated" date at the top of this page reflects the most recent change. Material changes will be announced on the website.

## 9. Contact

For privacy questions, data-rights requests, or any other matter concerning this policy, contact [entracte@drmowinckels.io](mailto:entracte@drmowinckels.io).
