// Type declarations for the plain-JS audit-gate table, so the tests get real
// checking instead of `any` behind a `@ts-expect-error`. The suppression is
// placement-fragile: prettier splits the import across lines and the directive
// stops applying to the line tsc reports the error on (see set-version.d.mts).

export type NonVacuity =
  | { kind: "count"; pattern: RegExp; floor: number; unit: string }
  | { kind: "native"; flag: string }
  | { kind: "exempt"; why: string };

export declare const GATES: Record<string, { nonVacuity: NonVacuity }>;

export declare function assess(
  gate: { name?: string; nonVacuity: NonVacuity },
  run: { status: number | null; output: string },
): { ok: boolean; exitCode: number; message?: string };
