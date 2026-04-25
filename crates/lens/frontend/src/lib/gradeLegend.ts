// Grade legend wording — product contract, not configurable via [site].
// Per SDD product-repositioning §3 Requirements 11 and 12, exact strings.
//
// Keeping this in a dedicated module so the Landing component reads from
// here (not from meta.features.site.*) — the SDD is explicit that grade
// wording stays product-owned even when an operator rebrands the apex.

export interface GradeLegendEntry {
  grade: string;
  descriptor: string;
  meaning: string;
}

export const GRADE_LEGEND: GradeLegendEntry[] = [
  { grade: 'A+', descriptor: 'excellent', meaning: 'ahead of most domains' },
  { grade: 'A',  descriptor: 'strong',    meaning: 'minor polish possible' },
  { grade: 'B',  descriptor: 'ok',        meaning: 'weaknesses worth fixing' },
  { grade: 'C',  descriptor: 'risky',     meaning: 'fix before your next audit' },
  { grade: 'D',  descriptor: 'broken',    meaning: 'users will notice' },
  { grade: 'F',  descriptor: 'critical',  meaning: 'fix today' },
];
