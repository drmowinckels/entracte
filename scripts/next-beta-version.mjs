#!/usr/bin/env node
// Works out which beta version comes next, from the list of existing tags.
//
// Betas are prereleases of the *next* patch: with 0.0.13 released, the betas
// leading to 0.0.14 are `v0.0.14-beta.1`, `-beta.2`, … Once 0.0.14 ships as a
// stable release the counter restarts against 0.0.15.
//
// Kept as a pure function over a tag list so the numbering can be tested
// without a git repo — getting this wrong means either overwriting a released
// tag or skipping the whole beta line.
//
// Usage:
//   git tag -l 'v*' | node scripts/next-beta-version.mjs
//   node scripts/next-beta-version.mjs --tags v0.0.13,v0.0.14-beta.1

const STABLE = /^v(\d+)\.(\d+)\.(\d+)$/;

/** `v0.0.14-beta.3` -> {base: "0.0.14", n: 3}; anything else -> null. */
function parseBeta(tag) {
  const m = /^v(\d+\.\d+\.\d+)-beta\.(\d+)$/.exec(tag);
  return m ? { base: m[1], n: Number(m[2]) } : null;
}

function cmpTriple(a, b) {
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
}

/**
 * The next beta version (no `v` prefix), given every existing tag.
 *
 * Throws when there is no stable release to base a beta on — a beta is
 * defined relative to the last stable, so guessing would be worse than
 * failing loudly.
 */
export function nextBetaVersion(tags) {
  const stables = tags
    .map((t) => STABLE.exec(t.trim()))
    .filter(Boolean)
    .map((m) => [Number(m[1]), Number(m[2]), Number(m[3])]);

  if (stables.length === 0) {
    throw new Error("no stable vMAJOR.MINOR.PATCH tag found; cannot derive the next beta");
  }

  stables.sort(cmpTriple);
  const [maj, min, patch] = stables[stables.length - 1];
  const base = `${maj}.${min}.${patch + 1}`;

  // Only betas for this base matter; betas for an already-released patch are
  // history and must not push the counter forward.
  const used = tags
    .map((t) => parseBeta(t.trim()))
    .filter((b) => b && b.base === base)
    .map((b) => b.n);

  const n = used.length === 0 ? 1 : Math.max(...used) + 1;
  return `${base}-beta.${n}`;
}

/** The tag that the next beta should be measured against for "any changes?". */
export function lastReleasedTag(tags) {
  const next = nextBetaVersion(tags);
  const base = next.slice(0, next.indexOf("-"));

  const betas = tags
    .map((t) => parseBeta(t.trim()))
    .filter((b) => b && b.base === base)
    .map((b) => b.n);

  if (betas.length > 0) return `v${base}-beta.${Math.max(...betas)}`;

  const stables = tags
    .map((t) => STABLE.exec(t.trim()))
    .filter(Boolean)
    .map((m) => [Number(m[1]), Number(m[2]), Number(m[3])]);
  stables.sort(cmpTriple);
  const [maj, min, patch] = stables[stables.length - 1];
  return `v${maj}.${min}.${patch}`;
}

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const invokedDirectly =
  process.argv[1] && resolve(fileURLToPath(import.meta.url)) === resolve(process.argv[1]);

if (invokedDirectly) {
  const args = process.argv.slice(2);
  const tagsIdx = args.indexOf("--tags");

  const collect = async () => {
    if (tagsIdx !== -1) return (args[tagsIdx + 1] ?? "").split(",");
    let buf = "";
    for await (const chunk of process.stdin) buf += chunk;
    return buf.split("\n");
  };

  collect()
    .then((tags) => {
      const list = tags.filter((t) => t.trim() !== "");
      const next = nextBetaVersion(list);
      const since = lastReleasedTag(list);
      if (args.includes("--since")) {
        console.log(since);
      } else {
        console.log(next);
      }
    })
    .catch((e) => {
      console.error(`error: ${e.message}`);
      process.exit(1);
    });
}
