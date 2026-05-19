# Supporter pack

::: warning Coming soon — work in progress
The supporter pack itself is wired end-to-end in the app, but the storefront infrastructure is still being set up. **Purchase and license activation are temporarily hidden** in the **About → Supporter** tab while we get the payment flow in place.

In the meantime the pack ships as source-only: every gated feature is in the codebase and unlocks for anyone running a build with a valid record, but there's no way to buy a key yet. Nothing in the free experience is affected.
:::

<div style="text-align: center; margin: 2rem 0 1rem;">
  <video controls preload="metadata" poster="/videos/entracte_supporter_poster.jpg" playsinline style="display: block; margin: 0 auto; max-width: 960px; width: 100%; border-radius: 8px; box-shadow: 0 4px 20px rgba(0,0,0,0.15);" aria-label="90-second tour of the supporter pack — custom themes, custom colour, editable break hints, custom CSS, custom sounds, and the license-removal flow.">
    <source src="/videos/entracte_supporter.mp4" type="video/mp4" />
    Your browser doesn't support inline video — <a href="/videos/entracte_supporter.mp4">download the clip</a> instead.
  </video>
</div>

Entracte is free and open source under Apache 2.0. If you'd like to support development, the supporter pack unlocks a few personalisation extras and helps keep the project moving.

It's intentionally light: nothing core depends on it. Every scheduling, suppression, profile, hooks, stats, accessibility, and CLI feature stays available to everyone, regardless of whether you have a supporter key.

## What's in it

| Tab                  | Setting             | What it unlocks                                                                                                                                     |
| -------------------- | ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Breaks → Overlay     | **Theme = Custom…** | Pick any colour via hex input or the native colour picker (with synchronised controls and an auto-darken cap so the overlay still dims the screen). |
| Breaks → Overlay     | **Theme = Rotate**  | A different preset per break, never the same one twice in a row.                                                                                    |
| Breaks → Break ideas | **Edit hint pools** | Add / remove / rewrite the prompts shown during a break. Mix selectors and rotation cadence remain free.                                            |
| Breaks → Custom CSS  | **Stylesheet**      | Freeform CSS injected into the settings window and the break overlay for full visual customisation.                                                 |
| Schedule → Sound     | **Custom file…**    | Point each break kind at your own audio file (end-chime or looping ambient).                                                                        |

Nothing in the scheduling, suppression, profile, hooks, stats, accessibility, or CLI surface is gated; the defaults remain usable forever. If your key later goes inactive you'll never lose access to a break you've configured — only the ability to edit personalisation while the key is inactive.

## How to get one

1. Open Entracte → tray menu → **About** → **Supporter** → **Become a supporter →**.
2. Complete checkout through the payment partner. They'll act as merchant of record, handling VAT and sales tax wherever you are.
3. The receipt email will contain your license key.
4. Back in **About → Supporter**, paste the key and click **Verify**.

The key is bound to the machine you activate it on. You can remove it from one machine and activate it on another at any time.

## How the key is stored and validated

Activation calls the license API and stores a small `supporter.json` in Entracte's app-data directory:

- **macOS** — `~/Library/Application Support/app.entracte/supporter.json`
- **Windows** — `%APPDATA%\app.entracte\supporter.json`
- **Linux** — `~/.config/app.entracte/supporter.json`

It's deliberately _not_ in `settings.json` — that file is often synced through dotfile managers, and a supporter key is machine-bound by design.

Entracte revalidates the key once a day in the background. If you're offline there's a 30-day grace window — flights, ferries, and conference Wi-Fi don't lock you out. If validation comes back invalid (refund, manual revocation), the local record is removed and the personalisation gates re-engage.

## Honour system, by design

The unlock check is plain source code, like the rest of Entracte. Someone determined to bypass it can. That's fine — the supporter pack is a way to fund the project and unlock a few niceties, not a digital lock. If you can't or don't want to pay, the free app is still a complete, useful product.

If you'd like to support without unlocking anything, you can also sponsor or donate through the channels listed on the [project page](https://github.com/drmowinckels/entracte).
