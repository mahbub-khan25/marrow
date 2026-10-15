// Reading and writing a note's YAML frontmatter without disturbing the rest
// of it: comments, key order and quoting stay as the person wrote them.

import { isMap, isScalar, parseDocument } from "yaml";

const FM = /^---\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/;

/** The frontmatter as plain values (empty if there is none or it's broken). */
export function props(text: string): Record<string, unknown> {
  const m = FM.exec(text);
  if (!m) return {};
  try {
    const doc = parseDocument(m[1]);
    if (doc.errors.length) return {};
    const v = doc.toJS() as unknown;
    return v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/**
 * Sets (or with `undefined`, removes) one key. Creates the frontmatter block
 * if the note has none. Returns the text unchanged if the YAML can't be read,
 * rather than risk mangling it.
 */
export function setProp(text: string, key: string, value: unknown): string {
  const m = FM.exec(text);
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  if (!m) {
    if (value === undefined) return text;
    const doc = parseDocument("{}");
    doc.contents = doc.createNode({ [key]: value }) as never;
    return `---${nl}${doc.toString().trimEnd()}${nl}---${nl}${text}`;
  }
  const doc = parseDocument(m[1]);
  if (doc.errors.length || (doc.contents !== null && !isMap(doc.contents))) return text;
  if (value === undefined) doc.delete(key);
  else doc.set(key, value);
  const body = doc.toString({ flowCollectionPadding: false, lineWidth: 0 }).trimEnd();
  const rest = text.slice(m[0].length);
  // An emptied block goes away entirely.
  if (!body || body === "{}") return rest;
  return `---${nl}${body.split("\n").join(nl)}${nl}---${nl}${rest}`;
}

/** Renames a key in place, keeping its value and position. */
export function renameProp(text: string, from: string, to: string): string {
  const m = FM.exec(text);
  if (!m || !to || from === to) return text;
  const doc = parseDocument(m[1]);
  if (doc.errors.length || !isMap(doc.contents)) return text;
  if (doc.has(to)) return text;
  const pair = doc.contents.items.find((p) => (isScalar(p.key) ? p.key.value : p.key) === from);
  if (!pair) return text;
  pair.key = doc.createNode(to) as never;
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  const body = doc.toString({ flowCollectionPadding: false, lineWidth: 0 }).trimEnd();
  return `---${nl}${body.split("\n").join(nl)}${nl}---${nl}${text.slice(m[0].length)}`;
}
