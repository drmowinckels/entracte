# Supporter pack

Entracte is free and open source under Apache 2.0. If you'd like to support development, the supporter pack unlocks a few personalisation extras and helps keep the project moving.

It's intentionally light: nothing core depends on it. Every scheduling, suppression, profile, hooks, stats, accessibility, and CLI feature stays available to everyone, regardless of whether you have a supporter key.

## What's in it

- **Custom overlay colour** — pick any hex or use the colour picker, on top of the built-in themes.
- **Theme rotation** — the `Rotate` option shuffles between your preset palettes break to break.
- **Editable break hints** — customise the prompts shown during a break, or rotate your own list.
- **Custom sounds** _(planned)_ — point each break kind at your own audio file.

The defaults remain available even after your key expires, so you'll never lose access to a break you've configured — only the ability to edit personalisation while the key is inactive.

## How to get one

1. Open Entracte → tray menu → **About** → **Supporter** → **Become a supporter →**.
2. Pay through Lemon Squeezy. They're the merchant of record, so they handle VAT and sales tax wherever you are.
3. The receipt email contains your license key.
4. Back in **About → Supporter**, paste the key and click **Verify**.

The key is bound to the machine you activate it on. You can remove it from one machine and activate it on another at any time.

## How the key is stored and validated

Activation calls the [Lemon Squeezy License API](https://docs.lemonsqueezy.com/api/license-keys) and stores a small `supporter.json` in Entracte's app-data directory:

- **macOS** — `~/Library/Application Support/app.entracte/supporter.json`
- **Windows** — `%APPDATA%\app.entracte\supporter.json`
- **Linux** — `~/.config/app.entracte/supporter.json`

It's deliberately _not_ in `settings.json` — that file is often synced through dotfile managers, and a supporter key is machine-bound by design.

Entracte revalidates the key once a day in the background. If you're offline there's a 30-day grace window — flights, ferries, and conference Wi-Fi don't lock you out. If validation comes back invalid (refund, manual revocation), the local record is removed and the personalisation gates re-engage.

## Honour system, by design

The unlock check is plain source code, like the rest of Entracte. Someone determined to bypass it can. That's fine — the supporter pack is a way to fund the project and unlock a few niceties, not a digital lock. If you can't or don't want to pay, the free app is still a complete, useful product.

If you'd like to support without unlocking anything, you can also sponsor or donate through the channels listed on the [project page](https://github.com/drmowinckels/entracte).
