import { defineConfig } from "vitepress";

export default defineConfig({
  title: "Entracte",
  description:
    "Cross-platform break reminder app, named after the theatre interval between acts.",
  base: "/entracte/",
  cleanUrls: true,
  lastUpdated: true,
  head: [
    ["link", { rel: "icon", href: "/entracte/logo.svg", type: "image/svg+xml" }],
    ["meta", { name: "theme-color", content: "#2e545c" }],
  ],
  themeConfig: {
    logo: "/logo.svg",
    nav: [
      { text: "Guide", link: "/guide/getting-started" },
      { text: "Architecture", link: "/architecture/" },
      { text: "Developer", link: "/developer/" },
      {
        text: "Releases",
        link: "https://github.com/drmowinckels/entracte/releases",
      },
    ],
    sidebar: {
      "/guide/": [
        {
          text: "Guide",
          items: [
            { text: "Getting started", link: "/guide/getting-started" },
            { text: "Why breaks?", link: "/guide/why-breaks" },
            { text: "Install", link: "/guide/install" },
            { text: "Settings", link: "/guide/settings" },
            { text: "Command line", link: "/guide/cli" },
            { text: "Supporter pack", link: "/guide/supporter" },
          ],
        },
      ],
      "/architecture/": [
        {
          text: "Architecture",
          items: [
            { text: "Overview", link: "/architecture/" },
            { text: "Scheduler", link: "/architecture/scheduler" },
            { text: "Per-OS detection", link: "/architecture/per-os" },
          ],
        },
      ],
      "/developer/": [
        {
          text: "Developer",
          items: [
            { text: "Overview", link: "/developer/" },
            { text: "Contributing", link: "/developer/contributing" },
            {
              text: "Architecture internals",
              link: "/developer/architecture-internals",
            },
            { text: "IPC contract", link: "/developer/ipc" },
          ],
        },
        {
          text: "API references",
          items: [
            { text: "Rust", link: "/developer/rust-api" },
            { text: "TypeScript", link: "/developer/ts-api" },
          ],
        },
      ],
    },
    socialLinks: [
      { icon: "github", link: "https://github.com/drmowinckels/entracte" },
    ],
    footer: {
      message: "Released under the Apache 2.0 License.",
      copyright: "Copyright © Athanasia Mowinckel",
    },
    search: { provider: "local" },
    editLink: {
      pattern: "https://github.com/drmowinckels/entracte/edit/main/docs/:path",
      text: "Edit this page on GitHub",
    },
  },
});
