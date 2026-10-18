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
  });
}
