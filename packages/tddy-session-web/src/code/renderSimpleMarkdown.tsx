import { createElement, type ReactNode } from "react";

/** `#` … `######` followed by a space: the heading's level and its text. */
const HEADING = /^(#{1,6}) (.*)$/;

const HEADING_CLASS = ["text-xl", "text-lg", "text-base", "text-sm", "text-sm", "text-sm"];

/** A fence line opening or closing a code block: three backticks, optionally naming a language. */
const isFence = (line: string) => line.trimStart().startsWith("```");

/** One block read from the lines: its node, and the index of the first line after it. */
type Block = { node: ReactNode; next: number };

/** Read the block starting at line `start` when it is of the reader's kind; `key` keys its node. */
type BlockReader = (lines: string[], start: number, key: number) => Block | null;

/** The line at `index`, without trailing whitespace; `""` past the end. */
const lineAt = (lines: string[], index: number) => (lines[index] ?? "").trimEnd();

const isListItem = (line: string) => line.startsWith("- ");

const readHeading: BlockReader = (lines, start, key) => {
  const heading = HEADING.exec(lineAt(lines, start));
  if (heading === null) return null;
  const level = heading[1].length;
  const className = `${HEADING_CLASS[level - 1]} font-semibold`;
  return { node: createElement(`h${level}`, { key, className }, heading[2]), next: start + 1 };
};

const readFence: BlockReader = (lines, start, key) => {
  if (!isFence(lineAt(lines, start))) return null;
  const code: string[] = [];
  let index = start + 1;
  while (index < lines.length && !isFence(lines[index] ?? "")) {
    code.push(lines[index] ?? "");
    index += 1;
  }
  const node = (
    <pre key={key} className="overflow-x-auto font-mono text-sm">
      <code>{code.join("\n")}</code>
    </pre>
  );
  // Past the closing fence, or past the end when it is missing.
  return { node, next: index + 1 };
};

const readList: BlockReader = (lines, start, key) => {
  const items: string[] = [];
  let index = start;
  for (let itemLine = lineAt(lines, index); isListItem(itemLine); itemLine = lineAt(lines, index)) {
    items.push(itemLine.slice(2));
    index += 1;
  }
  if (items.length === 0) return null;
  const node = (
    <ul key={key} className="list-disc pl-6">
      {items.map((item, itemIndex) => (
        <li key={itemIndex}>{item}</li>
      ))}
    </ul>
  );
  return { node, next: index };
};

const readParagraph = (lines: string[], start: number, key: number): Block => ({
  node: (
    <p key={key} className="whitespace-pre-wrap">
      {lineAt(lines, start)}
    </p>
  ),
  next: start + 1,
});

const BLOCK_READERS: BlockReader[] = [readHeading, readFence, readList];

/** The block starting at line `start`: the first reader that recognises it, else a paragraph. */
function readBlock(lines: string[], start: number, key: number): Block {
  for (const read of BLOCK_READERS) {
    const block = read(lines, start, key);
    if (block !== null) return block;
  }
  return readParagraph(lines, start, key);
}

/**
 * Minimal safe Markdown: block headings (`#` to `######`), fenced code blocks, lists, and
 * paragraphs. Inline/raw HTML is emitted as React text nodes (escaped), not parsed as DOM — no
 * script injection.
 *
 * Shared by the session workflow files panel, the worktree Code pane preview and its hover card.
 */
export function renderSimpleMarkdown(content: string): ReactNode {
  const lines = content.split(/\r?\n/);
  const blocks: ReactNode[] = [];
  let index = 0;
  while (index < lines.length) {
    if (lineAt(lines, index) === "") {
      index += 1;
      continue;
    }
    const block = readBlock(lines, index, blocks.length);
    blocks.push(block.node);
    index = block.next;
  }
  return <div className="workflow-md-preview space-y-2">{blocks}</div>;
}
