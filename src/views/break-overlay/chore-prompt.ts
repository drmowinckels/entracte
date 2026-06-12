/** Compose the long-break chore nudge shown in the wellness-hint space.
 *  The backend sends the raw task text (`chore_prompt`); the overlay frames
 *  it with the break length so it reads as an invitation rather than an
 *  order — "you've got ~10 min, knock out X". Sub-minute breaks (which
 *  shouldn't normally carry a chore) fall back to a length-free phrasing. */
export function choreNudge(chore: string, durationSecs: number): string {
  const mins = Math.round(durationSecs / 60);
  if (mins >= 1) {
    return `You've got ~${mins} min — knock out: ${chore}`;
  }
  return `Quick one — knock out: ${chore}`;
}
