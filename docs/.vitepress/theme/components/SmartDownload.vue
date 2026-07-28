<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  classify,
  detectOs,
  groupByOs,
  OS_LABELS,
  selectPrimary,
  selectSecondary,
  type Installer,
  type Os,
} from "./download-detect";

// Auto-detecting download control for the landing + install pages. It
// asks the GitHub API for the latest release, detects the visitor's OS
// (and arch where the browser exposes it), and offers a one-click
// download of the matching installer — with every other build one
// "All downloads" toggle away. On any failure it degrades to a plain
// link to the Releases page. The pure detection/matching logic lives in
// ./download-detect.ts (unit-tested); this file is DOM + fetch glue.

const RELEASES_PAGE = "https://github.com/drmowinckels/entracte/releases";
const LATEST_API =
  "https://api.github.com/repos/drmowinckels/entracte/releases/latest";

async function detectArch(): Promise<"aarch64" | "x64" | null> {
  // Only Chromium exposes CPU architecture (via UA-CH). Safari/Firefox
  // mask it, so macOS visitors there see both builds instead.
  const uaData = (
    navigator as Navigator & {
      userAgentData?: {
        getHighEntropyValues?: (h: string[]) => Promise<{ architecture?: string }>;
      };
    }
  ).userAgentData;
  if (!uaData?.getHighEntropyValues) return null;
  try {
    const { architecture } = await uaData.getHighEntropyValues(["architecture"]);
    if (architecture === "arm") return "aarch64";
    if (architecture === "x86") return "x64";
    return null;
  } catch {
    return null;
  }
}

// Module-level cache so navigating between the landing and install pages
// doesn't re-hit the API (and its unauthenticated rate limit).
let releaseCache: Promise<{ tag: string; installers: Installer[] }> | null = null;

function loadRelease() {
  if (!releaseCache) {
    releaseCache = fetch(LATEST_API, {
      headers: { Accept: "application/vnd.github+json" },
    })
      .then((r) => {
        if (!r.ok) throw new Error(`GitHub API ${r.status}`);
        return r.json();
      })
      .then(
        (data: {
          tag_name?: string;
          assets?: { name: string; browser_download_url: string }[];
        }) => ({
          tag: data.tag_name ?? "",
          installers: classify(
            (data.assets ?? []).map((a) => ({ name: a.name, url: a.browser_download_url })),
          ),
        }),
      )
      .catch((e) => {
        releaseCache = null; // let a later mount retry
        throw e;
      });
  }
  return releaseCache;
}

const status = ref<"loading" | "ready" | "error">("loading");
const tag = ref("");
const installers = ref<Installer[]>([]);
const os = ref<Os>("other");
const arch = ref<"aarch64" | "x64" | null>(null);

onMounted(async () => {
  os.value = detectOs(navigator.userAgent);
  arch.value = await detectArch();
  try {
    const rel = await loadRelease();
    tag.value = rel.tag;
    installers.value = rel.installers;
    status.value = "ready";
  } catch {
    status.value = "error";
  }
});

const primary = computed(() => selectPrimary(installers.value, os.value, arch.value));
const secondary = computed(() =>
  selectSecondary(installers.value, os.value, primary.value),
);
const grouped = computed(() => groupByOs(installers.value));
const osLabel = computed(() => OS_LABELS[os.value]);
</script>

<template>
  <div class="smart-download">
    <div v-if="status === 'loading'" class="sd-line">
      <a class="sd-btn" :href="RELEASES_PAGE" target="_blank" rel="noreferrer">
        Download Entracte
      </a>
      <span class="sd-note">Detecting your system…</span>
    </div>

    <template v-else-if="status === 'ready'">
      <div class="sd-primary">
        <a v-for="i in primary" :key="i.name" class="sd-btn" :href="i.url">
          Download for {{ osLabel
          }}<template v-if="os === 'macos'"> · {{ i.label }}</template>
        </a>
        <a
          v-if="!primary.length"
          class="sd-btn"
          :href="RELEASES_PAGE"
          target="_blank"
          rel="noreferrer"
        >
          Download Entracte
        </a>
      </div>

      <p class="sd-meta">
        <template v-if="tag">Latest release <strong>{{ tag }}</strong>.</template>
        <template v-if="primary.length && secondary.length">
          Other {{ osLabel }} builds:
          <template v-for="(i, idx) in secondary" :key="i.name"
            ><a :href="i.url">{{ i.label }}</a
            ><template v-if="idx < secondary.length - 1"> · </template></template
          >.
        </template>
      </p>

      <details v-if="grouped.length" class="sd-details">
        <summary>All platforms &amp; formats</summary>
        <div class="sd-groups">
          <div v-for="g in grouped" :key="g.os" class="sd-group">
            <span class="sd-group-title">{{ g.label }}</span>
            <ul>
              <li v-for="i in g.items" :key="i.name">
                <a :href="i.url">{{ i.label }}</a>
              </li>
            </ul>
          </div>
        </div>
      </details>
    </template>

    <div v-else class="sd-line">
      <a class="sd-btn" :href="RELEASES_PAGE" target="_blank" rel="noreferrer">
        Download from Releases
      </a>
      <span class="sd-note">
        Couldn't reach GitHub just now — pick your build on the
        <a :href="RELEASES_PAGE" target="_blank" rel="noreferrer">Releases page</a>.
      </span>
    </div>
  </div>
</template>

<style scoped>
.smart-download {
  margin: 1.5rem 0;
}
.sd-primary {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}
.sd-btn {
  display: inline-block;
  padding: 0.5rem 1.25rem;
  border-radius: 8px;
  font-weight: 600;
  font-size: 0.95rem;
  color: var(--vp-button-brand-text);
  background: var(--vp-button-brand-bg);
  border: 1px solid var(--vp-button-brand-border);
  transition:
    background-color 0.2s,
    border-color 0.2s;
}
.sd-btn:hover {
  text-decoration: none;
  color: var(--vp-button-brand-hover-text);
  background: var(--vp-button-brand-hover-bg);
  border-color: var(--vp-button-brand-hover-border);
}
.sd-line {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
}
.sd-note,
.sd-meta {
  font-size: 0.85rem;
  color: var(--vp-c-text-2);
}
.sd-meta {
  margin-top: 0.75rem;
}
.sd-details summary {
  cursor: pointer;
  color: var(--vp-c-brand-1);
  font-size: 0.85rem;
}
.sd-groups {
  display: flex;
  flex-wrap: wrap;
  gap: 1.5rem;
  margin-top: 0.75rem;
}
.sd-group-title {
  font-weight: 600;
  color: var(--vp-c-text-1);
}
.sd-group ul {
  margin: 0.25rem 0 0;
  padding-left: 1rem;
}
</style>
