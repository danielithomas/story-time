// Display helpers.

const numbers = new Intl.NumberFormat();

export const formatNumber = (n: number) => numbers.format(n);

/** "+1,203" or "-40". */
export const signed = (n: number) => (n > 0 ? "+" : "") + formatNumber(n);

export function plural(n: number, one: string, many = `${one}s`) {
  return `${formatNumber(n)} ${n === 1 ? one : many}`;
}

export function duration(minutes: number) {
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest} min`;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

export const chapterNumber = (n: number | null) => (n == null ? "" : String(n).padStart(2, "0"));

/** "[[Mara Chen|Mara]]" reads as "Mara"; "[[Mara Chen]]" as "Mara Chen". */
export const plainLinks = (text: string) =>
  text.replace(/\[\[([^\]|]*)(?:\|([^\]]*))?\]\]/g, (_, target: string, alias?: string) => alias ?? target);

const escapes: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

/** A line of Markdown as safe HTML: everything is escaped first, then `code`, **strong** and *emphasis* are styled. */
export function inlineMarkdown(text: string): string {
  return text
    .replace(/[&<>"']/g, (c) => escapes[c])
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^\w*])\*([^*\s][^*]*?)\*(?!\w)/g, "$1<em>$2</em>")
    .replace(/(^|[^\w])_([^_\s][^_]*?)_(?!\w)/g, "$1<em>$2</em>");
}

export const capitalise = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);
