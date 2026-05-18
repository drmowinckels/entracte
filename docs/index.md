---
layout: home

hero:
  name: Entracte
  text: Breaks that respect your flow.
  tagline: A cross-platform break reminder app, named after the theatre interval between acts. Pronounced "ahn-TRAHKT", not "en-tract".
  image:
    src: /logo_gradient.svg
    alt: Entracte logo
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: Why breaks?
      link: /guide/why-breaks
    - theme: alt
      text: Install
      link: /guide/install
    - theme: alt
      text: View on GitHub
      link: https://github.com/drmowinckels/entracte

features:
  - icon: 🎭
    title: Three kinds of break
    details: Micro breaks for eyes and posture, Long breaks for real rest, and a Sleep prompt during your bedtime window.
    link: /guide/why-breaks
    linkText: Why each kind matters
  - icon: 🤫
    title: Knows when to stay quiet
    details: Skips when Do Not Disturb is on, your camera is in use, you've gone idle, or you're outside your work hours. One borderless overlay per monitor when it does fire.
  - icon: 👤
    title: Profiles for every mode
    details: Switch from the tray between "Deep work", "Meetings", "Weekend"… each carries its own intervals, hint pool, and sound. Inherit-from-active when you create a new one.
  - icon: 💭
    title: Wellness hints, not nag screens
    details: Each break shows one of a rotating pool of prompts — physical stretches, breathing cues, social nudges. Edit the pool, mix the categories, swap in your own.
  - icon: 📊
    title: Insights you can actually use
    details: 84-day heatmap, time-of-day distribution, suppression-reason breakdown, daily screen-time budget with reminders. Export to CSV when you want it elsewhere.
  - icon: 🦀
    title: Native and lightweight
    details: Rust + Tauri 2 core. Small binary, low idle CPU, no Electron. Local-only — no telemetry, no cloud sync, no account.
---

<div style="text-align: center; margin: 3rem 0 1rem;">
  <img src="/screenshots/break-overlay-active.png" alt="A micro break overlay: countdown ring at 10 seconds with the prompt 'Sip some water.' and Postpone / Skip buttons" style="max-width: 720px; width: 100%; border-radius: 8px; box-shadow: 0 4px 20px rgba(0,0,0,0.15);" />
</div>

::: warning Pre-release
No tagged binaries yet — installing currently means [building from source](/guide/install). The release pipeline is in place; the first `v0.1.0` will land on the [Releases page](https://github.com/drmowinckels/entracte/releases) once it's cut.
:::

<div style="text-align: center; margin-top: 2rem;">

## How to say it

"ahn-TRAHKT" — French, IPA `/ɑ̃.tʁakt/`. Not "en-tract".

<audio controls preload="none" src="/audio/entracte-fr.ogg" style="display: block; margin: 0.5rem auto 0;">Your browser doesn't support inline audio — <a href="/audio/entracte-fr.ogg">download the clip</a> instead.</audio>

<sub>Recording by Vion Nicolas via the <a href="https://commons.wikimedia.org/wiki/File:Fr-entracte.ogg">Shtooka Project</a>, used under <a href="https://creativecommons.org/licenses/by/2.0/fr/deed.en">CC BY 2.0 France</a>.</sub>

</div>
