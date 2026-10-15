// Embeds you can edit in place ("synced blocks"). `![[Note#^id]]` shows one
// block of another note, `![[Note#Heading]]` a section, `![[Note]]` the whole
// note. Editing the embed changes the original, so every place that embeds it
// shows the change. Block ids are Obsidian's: ` ^id` at the end of a line, or
// on a line of its own after a table, quote or code block.

import { headings } from "./util";

export interface EmbedRange {
  /** Lines of the target note shown by the embed. */
  start: number;
  end: number;
  /** The block id, if it sits at the end of a line inside the range. */
  inlineId?: { id: string; line: number };
}

const LIST = /^(\s*)([-*+]|\d+[.)])\s/;
const idAtEnd = (id: string) => new RegExp(`(^|\\s)\\^${id.replace(/[^A-Za-z0-9-]/g, "")}\\s*$`);

/** Where a block id, heading or whole note sits in `text`. */
export function embedRange(text: string, sub: string): EmbedRange | null {
  const lines = text.split("\n");
  if (!sub) {
    // The whole note, without its frontmatter.
    let start = 0;
    if (lines[0]?.trim() === "---") {
      const close = lines.findIndex((l, i) => i > 0 && (l.trim() === "---" || l.trim() === "..."));
      if (close > 0) start = close + 1;
    }
    return { start, end: lines.length };
  }
  if (sub.startsWith("^")) {
    const id = sub.slice(1);
    const re = idAtEnd(id);
    const i = lines.findIndex((l) => re.test(l));
    if (i < 0) return null;
    if (lines[i].trim() === `^${id}`) {
      // Own line: the block above it.
      let last = i - 1;
      while (last >= 0 && !lines[last].trim()) last--;
      if (last < 0) return null;
      let start = last;
      while (start > 0 && lines[start - 1].trim()) start--;
      return { start, end: last + 1 };
    }
    const m = LIST.exec(lines[i]);
    if (m) {
      // A list item and the lines nested under it.
      let end = i + 1;
      while (end < lines.length && lines[end].trim() && /^\s*/.exec(lines[end])![0].length > m[1].length) end++;
      return { start: i, end, inlineId: { id, line: i } };
    }
    // A paragraph: up to the blank line (or heading/list) above.
    let start = i;
    while (start > 0 && lines[start - 1].trim() && !/^#{1,6}\s/.test(lines[start - 1]) && !LIST.test(lines[start - 1])) start--;
    return { start, end: i + 1, inlineId: { id, line: i } };
  }
  const want = sub.trim().toLowerCase();
  const hs = headings(text);
  const k = hs.findIndex((x) => x.text.trim().toLowerCase() === want);
  if (k < 0) return null;
  const next = hs.slice(k + 1).find((x) => x.level <= hs[k].level);
  return { start: hs[k].line, end: next ? next.line : lines.length };
}

/** The embed's text for editing (the block id hidden, so it can't be lost). */
export function rangeText(text: string, r: EmbedRange): string {
  const lines = text.split("\n").slice(r.start, r.end);
  if (r.inlineId) {
    const i = r.inlineId.line - r.start;
    lines[i] = lines[i].replace(idAtEnd(r.inlineId.id), "");
  }
  return lines.join("\n").replace(/\n+$/, "");
}

/** `text` with the embed's lines replaced by `edited` (its block id kept). */
export function replaceRange(text: string, r: EmbedRange, edited: string): string {
  const lines = text.split("\n");
  const trailing: string[] = [];
  // Keep the blank lines that separated the range from what follows.
  for (let i = r.end - 1; i > r.start && !lines[i].trim(); i--) trailing.unshift("");
  const next = edited.replace(/\n+$/, "").split("\n");
  if (r.inlineId) {
    const isList = LIST.test(lines[r.inlineId.line]);
    // A list item keeps its id on its first line; a paragraph on its last.
    let at = isList ? 0 : next.length - 1;
    while (at > 0 && !next[at].trim()) at--;
    next[at] = `${next[at].replace(/\s+$/, "")} ^${r.inlineId.id}`;
  }
  lines.splice(r.start, r.end - r.start, ...next, ...trailing);
  return lines.join("\n");
}

/** A short random id, like Obsidian makes. */
export function newBlockId(): string {
  const a = "abcdefghijklmnopqrstuvwxyz0123456789";
  let s = "";
  for (let i = 0; i < 6; i++) s += a[Math.floor(Math.random() * a.length)];
  return s;
}

/**
 * Gives a block (lines start..end) an id, or finds the one it has.
 * Returns the new text and the id.
 */
export function ensureBlockId(text: string, start: number, end: number, kind: "li" | "p" | "other"): { text: string; id: string } {
  const lines = text.split("\n");
  const find = (l: string | undefined) => (l ? /(?:^|\s)\^([A-Za-z0-9-]+)\s*$/.exec(l)?.[1] : undefined);
  if (kind === "li") {
    const id = find(lines[start]);
    if (id) return { text, id };
    const nid = newBlockId();
    lines[start] = `${lines[start].replace(/\s+$/, "")} ^${nid}`;
    return { text: lines.join("\n"), id: nid };
  }
  if (kind === "p") {
    const id = find(lines[end - 1]);
    if (id) return { text, id };
    const nid = newBlockId();
    lines[end - 1] = `${lines[end - 1].replace(/\s+$/, "")} ^${nid}`;
    return { text: lines.join("\n"), id: nid };
  }
  // Tables, quotes, code: the id goes on its own line after a blank line.
  let j = end;
  while (j < lines.length && !lines[j].trim()) j++;
  const own = lines[j]?.trim().match(/^\^([A-Za-z0-9-]+)$/);
  if (own) return { text, id: own[1] };
  const nid = newBlockId();
  lines.splice(end, 0, "", `^${nid}`);
  if (end + 2 < lines.length && lines[end + 2].trim()) lines.splice(end + 2, 0, "");
  return { text: lines.join("\n"), id: nid };
}
