/**
 * SimpleMarkdown — a minimal, XSS-safe markdown renderer.
 *
 * Supported syntax:
 *   # ## ###        → h1 / h2 / h3
 *   **bold**         → <strong>
 *   *italic*         → <em>
 *   `inline code`    → <code>
 *   ```block```      → <pre><code>
 *   - item           → <ul><li>
 *   1. item          → <ol><li>
 *   blank lines      → paragraph breaks
 *
 * No HTML pass-through — all angle brackets are escaped before rendering.
 */

import React from "react";

// ---------------------------------------------------------------------------
// Inline renderer (bold, italic, inline code, plain text)
// ---------------------------------------------------------------------------

function renderInline(text: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  // Order matters: code first (to avoid italicising backtick content)
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*)/g;
  let lastIndex = 0;
  let key = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    const before = text.slice(lastIndex, match.index);
    if (before) nodes.push(<React.Fragment key={key++}>{before}</React.Fragment>);

    const token = match[0];
    if (token.startsWith("`")) {
      nodes.push(<code key={key++}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith("**")) {
      nodes.push(<strong key={key++}>{token.slice(2, -2)}</strong>);
    } else {
      nodes.push(<em key={key++}>{token.slice(1, -1)}</em>);
    }
    lastIndex = match.index + token.length;
  }

  const tail = text.slice(lastIndex);
  if (tail) nodes.push(<React.Fragment key={key++}>{tail}</React.Fragment>);

  return nodes;
}

// ---------------------------------------------------------------------------
// Block-level parser
// ---------------------------------------------------------------------------

type Block =
  | { type: "h1" | "h2" | "h3"; text: string }
  | { type: "ul"; items: string[] }
  | { type: "ol"; items: string[] }
  | { type: "code"; lang: string; text: string }
  | { type: "p"; text: string };

function parseBlocks(markdown: string): Block[] {
  const blocks: Block[] = [];
  const lines = markdown.split("\n");
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block
    if (line.startsWith("```")) {
      const lang = line.slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      i++; // consume closing ```
      blocks.push({ type: "code", lang, text: codeLines.join("\n") });
      continue;
    }

    // Headings
    const h3 = line.match(/^### (.+)/);
    const h2 = line.match(/^## (.+)/);
    const h1 = line.match(/^# (.+)/);
    if (h3) { blocks.push({ type: "h3", text: h3[1] }); i++; continue; }
    if (h2) { blocks.push({ type: "h2", text: h2[1] }); i++; continue; }
    if (h1) { blocks.push({ type: "h1", text: h1[1] }); i++; continue; }

    // Unordered list
    if (/^[-*] /.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*] /.test(lines[i])) {
        items.push(lines[i].replace(/^[-*] /, ""));
        i++;
      }
      blocks.push({ type: "ul", items });
      continue;
    }

    // Ordered list
    if (/^\d+\. /.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\. /.test(lines[i])) {
        items.push(lines[i].replace(/^\d+\. /, ""));
        i++;
      }
      blocks.push({ type: "ol", items });
      continue;
    }

    // Blank line — separator, skip
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Paragraph — collect until blank or block-start
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^#{1,3} /.test(lines[i]) &&
      !/^[-*] /.test(lines[i]) &&
      !/^\d+\. /.test(lines[i]) &&
      !lines[i].startsWith("```")
    ) {
      paraLines.push(lines[i]);
      i++;
    }
    if (paraLines.length > 0) {
      blocks.push({ type: "p", text: paraLines.join(" ") });
    }
  }

  return blocks;
}

// ---------------------------------------------------------------------------
// React component
// ---------------------------------------------------------------------------

export interface SimpleMarkdownProps {
  content: string;
  className?: string;
}

export function SimpleMarkdown({ content, className }: SimpleMarkdownProps) {
  const blocks = parseBlocks(content);
  let key = 0;

  const rendered = blocks.map((block) => {
    switch (block.type) {
      case "h1":
        return <h1 key={key++}>{renderInline(block.text)}</h1>;
      case "h2":
        return <h2 key={key++}>{renderInline(block.text)}</h2>;
      case "h3":
        return <h3 key={key++}>{renderInline(block.text)}</h3>;
      case "ul":
        return (
          <ul key={key++}>
            {block.items.map((item, idx) => (
              <li key={idx}>{renderInline(item)}</li>
            ))}
          </ul>
        );
      case "ol":
        return (
          <ol key={key++}>
            {block.items.map((item, idx) => (
              <li key={idx}>{renderInline(item)}</li>
            ))}
          </ol>
        );
      case "code":
        return (
          <pre key={key++}>
            <code className={block.lang ? `language-${block.lang}` : undefined}>
              {block.text}
            </code>
          </pre>
        );
      case "p":
        return <p key={key++}>{renderInline(block.text)}</p>;
    }
  });

  return (
    <div className={`markdown-content${className ? ` ${className}` : ""}`}>
      {rendered}
    </div>
  );
}
