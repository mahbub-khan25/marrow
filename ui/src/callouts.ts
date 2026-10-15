// Callout types: the ones Obsidian knows, with an icon and a colour each.

export interface CalloutKind {
  type: string;
  label: string;
  icon: string;
  color: string;
}

export const CALLOUTS: CalloutKind[] = [
  { type: "note", label: "Note", icon: "pencil-simple-line", color: "var(--co-blue)" },
  { type: "info", label: "Info", icon: "info", color: "var(--co-blue)" },
  { type: "tip", label: "Tip", icon: "lightbulb", color: "var(--co-teal)" },
  { type: "success", label: "Success", icon: "check-circle", color: "var(--co-green)" },
  { type: "question", label: "Question", icon: "question", color: "var(--co-yellow)" },
  { type: "warning", label: "Warning", icon: "warning", color: "var(--co-orange)" },
  { type: "danger", label: "Danger", icon: "lightning", color: "var(--co-red)" },
  { type: "bug", label: "Bug", icon: "bug", color: "var(--co-red)" },
  { type: "example", label: "Example", icon: "list-bullets", color: "var(--co-violet)" },
  { type: "quote", label: "Quote", icon: "quotes", color: "var(--co-grey)" },
];

/** Other names Obsidian accepts for the same kinds. */
const ALIASES: Record<string, string> = {
  abstract: "example",
  summary: "example",
  tldr: "example",
  todo: "info",
  hint: "tip",
  important: "tip",
  check: "success",
  done: "success",
  help: "question",
  faq: "question",
  caution: "warning",
  attention: "warning",
  failure: "danger",
  fail: "danger",
  missing: "danger",
  error: "danger",
  cite: "quote",
};

export function calloutKind(type: string): CalloutKind {
  const t = type.toLowerCase();
  const key = ALIASES[t] ?? t;
  if (t === "summary" || t === "abstract" || t === "tldr") {
    return { type: t, label: "Summary", icon: "clipboard-text", color: "var(--co-teal)" };
  }
  return CALLOUTS.find((c) => c.type === key) ?? CALLOUTS[0];
}
