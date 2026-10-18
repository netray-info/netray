export function countdown(el: HTMLElement): void {
  const button = document.querySelector<HTMLButtonElement>('#recheck button[name="trigger"]');
  let left = Number.parseInt(el.getAttribute("data-retry-after") ?? "0", 10);
  if (!(left > 0)) {
    button?.removeAttribute("disabled");
    return;
  }
  button?.setAttribute("disabled", "");
  const deadline = Date.now() + left * 1000;
  const timer = setInterval(() => {
    left = Math.max(0, Math.min(left - 1, Math.ceil((deadline - Date.now()) / 1000)));
    el.textContent = String(left);
    el.setAttribute("data-retry-after", String(left));
    if (left <= 0) {
      clearInterval(timer);
      button?.removeAttribute("disabled");
    }
  }, 1000);
}
