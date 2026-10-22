export function buildQuery(
  domain: string,
  types: string[],
  resolver: string | null,
  job: string | null,
): string {
  return [
    domain,
    ...types,
    resolver ? "@" + resolver : "",
    job && job !== "query" ? "+" + job : "",
  ]
    .filter(Boolean)
    .join(" ");
}

export function dns(el: HTMLElement): void {
  const form = el.closest("form") ?? el.parentElement?.querySelector("form");
  const q = form?.querySelector<HTMLInputElement>("input[name=q]");
  if (!q) return;
  // The +check note shows while the query holds the token (spec v2-ui-polish R3).
  const note = el.querySelector<HTMLElement>(".dns-builder__note");
  const syncNote = (): void => {
    // the parser reads flags in any case
    if (note) note.hidden = !q.value.split(/\s+/).some((t) => t.toLowerCase() === "+check");
  };
  q.addEventListener("input", syncNote);
  // The shell fills and clears the field without an input event: re-read it after each
  // interaction with the shell, once the shell's own handlers ran.
  const later = (): void => void setTimeout(syncNote, 0);
  const shell = el.closest(".inspector-shell") ?? form ?? document;
  for (const type of ["click", "keyup", "submit"]) shell.addEventListener(type, later);
  window.addEventListener("popstate", later);
  el.addEventListener("change", () => {
    const domain = q.value.trim().split(/\s+/)[0] ?? "";
    if (!domain) return;
    const types = Array.from(el.querySelectorAll<HTMLInputElement>("input[name=dns-type]"))
      .filter((box) => box.checked)
      .map((box) => box.value);
    const job =
      Array.from(el.querySelectorAll<HTMLInputElement>("input[name=dns-job]")).find(
        (radio) => radio.checked,
      )?.value ?? null;
    const resolver = el.querySelector<HTMLSelectElement>("select[name=dns-resolver]")?.value ?? null;
    q.value = buildQuery(domain, types, resolver, job);
    syncNote();
  });
}
