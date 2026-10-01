// Type declarations for the plain-JS release scripts, so the tests get real
// checking instead of `any` behind a `@ts-expect-error`. The suppression was
// placement-fragile: prettier split the import across lines and the directive
// stopped applying to the line tsc reports the error on.

export declare const VERSION_RE: RegExp;

export declare function edits(
  version: string,
  pkgName: string,
): { file: string; find: RegExp; replace: string; count: number }[];

export declare function applyVersion(
  root: string,
  version: string,
  options?: { check?: boolean },
): string[];
