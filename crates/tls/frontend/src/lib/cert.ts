/** Extract CN= from a DN string, falling back to the full string. */
export function certDisplayName(dn: string): string {
  const m = dn.match(/CN=([^,]+)/);
  return m ? m[1].trim() : dn;
}
