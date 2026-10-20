export function shell(el: HTMLElement): void {
  const form = el.querySelector("form");
  const input = el.querySelector<HTMLInputElement>(".query-input__field");
  const clear = el.querySelector<HTMLElement>(".query-input__clear");
  if (!form || !input || !clear) return;
  const sync = (): void => {
    clear.hidden = input.value === "";
  };
  sync();
  input.addEventListener("input", sync);
  clear.addEventListener("click", () => {
    input.value = "";
    sync();
    input.focus();
  });
  el.addEventListener("click", (ev) => {
    const chip = (ev.target as Element | null)?.closest<HTMLElement>(
      "a.inspector-examples__chip[data-example]",
    );
    if (!chip || !el.contains(chip)) return;
    if (ev.button !== 0 || ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey) return;
    ev.preventDefault();
    input.value = chip.dataset.example ?? "";
    sync();
    form.requestSubmit();
  });
}
