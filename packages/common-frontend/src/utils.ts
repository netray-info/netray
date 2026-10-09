/**
 * Browser utility helpers shared across the netray.info frontends.
 */

/**
 * Trigger a file download in the browser.
 */
export function downloadFile(content: string, filename: string, mime: string): void {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

/**
 * Write text to the clipboard. Returns true on success, false on failure.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * Wrap a value in a CommonMark inline code span. The fence is one backtick
 * longer than the longest backtick run in the value. Line endings (`\r\n`,
 * `\r`, `\n`) become one space; an empty value yields ''. Callers escape `|`
 * themselves.
 */
export function inlineCode(input: string): string {
  const value = input.replace(/\r\n|\r|\n/g, ' ');
  if (value === '') return '';
  let longest = 0;
  for (const run of value.match(/`+/g) ?? []) longest = Math.max(longest, run.length);
  const fence = '`'.repeat(longest + 1);
  const edgeTick = value.startsWith('`') || value.endsWith('`');
  const spaced = value.startsWith(' ') && value.endsWith(' ') && value.trim() !== '';
  const pad = edgeTick || spaced ? ' ' : '';
  return `${fence}${pad}${value}${pad}${fence}`;
}
