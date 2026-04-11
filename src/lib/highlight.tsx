import type { ReactNode } from "react";

/**
 * Splits `text` into plain and <span class="hl">...</span> segments based on
 * case-insensitive matches of `query`. Returns the original text unchanged
 * when no query or no matches are found.
 */
export function highlightText(text: string, query: string): ReactNode {
  if (!query) return text;
  const lower = text.toLowerCase();
  const q = query.toLowerCase();
  if (!lower.includes(q)) return text;
  const parts: ReactNode[] = [];
  let i = 0;
  let idx: number;
  let keyCounter = 0;
  while ((idx = lower.indexOf(q, i)) !== -1) {
    if (idx > i) parts.push(text.slice(i, idx));
    parts.push(
      <span key={`hl-${keyCounter++}`} className="hl">
        {text.slice(idx, idx + q.length)}
      </span>,
    );
    i = idx + q.length;
  }
  if (i < text.length) parts.push(text.slice(i));
  return parts;
}
