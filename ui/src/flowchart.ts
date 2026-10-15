// A Mermaid flowchart as data, so it can be edited visually: boxes (with a
// label and a shape) and arrows (with an optional label and a line style).
// Lines this doesn't understand (styles, classes, comments, clicks) are kept
// as they are. Charts with subgraphs are left to the code view.

export type Shape = "rect" | "round" | "stadium" | "circle" | "diamond" | "hexagon" | "cylinder" | "subroutine";
export type Line = "arrow" | "line" | "dotted" | "thick";
export type Dir = "LR" | "TD" | "RL" | "BT";

export interface FNode {
  id: string;
  label: string;
  shape: Shape;
}

export interface FEdge {
  from: string;
  to: string;
  label: string;
  line: Line;
}

export interface Flow {
  keyword: "flowchart" | "graph";
  dir: Dir;
  nodes: FNode[];
  edges: FEdge[];
  /** Lines kept as written (styles, classDefs, comments…). */
  extra: string[];
}

export const SHAPES: [Shape, string, string, string][] = [
  // shape, name, open, close
  ["rect", "Box", "[", "]"],
  ["round", "Rounded", "(", ")"],
  ["stadium", "Pill", "([", "])"],
  ["circle", "Circle", "((", "))"],
  ["diamond", "Decision", "{", "}"],
  ["hexagon", "Hexagon", "{{", "}}"],
  ["cylinder", "Database", "[(", ")]"],
  ["subroutine", "Subroutine", "[[", "]]"],
];

export const LINES: [Line, string, string][] = [
  ["arrow", "Arrow", "-->"],
  ["line", "Line", "---"],
  ["dotted", "Dotted", "-.->"],
  ["thick", "Thick", "==>"],
];

// Longest openers first, so "((" wins over "(".
const SHAPE_RE: [Shape, RegExp][] = [
  ["circle", /^\(\((.*?)\)\)/],
  ["stadium", /^\(\[(.*?)\]\)/],
  ["cylinder", /^\[\((.*?)\)\]/],
  ["subroutine", /^\[\[(.*?)\]\]/],
  ["hexagon", /^\{\{(.*?)\}\}/],
  ["rect", /^\[(.*?)\]/],
  ["round", /^\((.*?)\)/],
  ["diamond", /^\{(.*?)\}/],
];

const ID = /^[A-Za-z0-9_À-￿][\wÀ-￿-]*/;

const unquote = (s: string) => {
  const t = s.trim();
  return t.length >= 2 && t.startsWith('"') && t.endsWith('"') ? t.slice(1, -1).replace(/#quot;/g, '"') : t;
};

/** Reads a link at the start of `s`: its style, label and length. */
function readLink(s: string): { line: Line; label: string; len: number } | null {
  let m: RegExpExecArray | null;
  // A -- text --> B / A -. text .-> B / A == text ==> B
  if ((m = /^\s*--\s+([^-|>][^>]*?)\s+-->\s*/.exec(s))) return { line: "arrow", label: m[1], len: m[0].length };
  if ((m = /^\s*-\.\s+(.+?)\s+\.->\s*/.exec(s))) return { line: "dotted", label: m[1], len: m[0].length };
  if ((m = /^\s*==\s+(.+?)\s+==>\s*/.exec(s))) return { line: "thick", label: m[1], len: m[0].length };
  // A -->|text| B and plain links
  if ((m = /^\s*(-{2,}>|-\.+->|={2,}>|-{3,})\s*(?:\|([^|]*)\|)?\s*/.exec(s))) {
    const op = m[1];
    const line: Line = op.startsWith("=") ? "thick" : op.includes(".") ? "dotted" : op.endsWith(">") ? "arrow" : "line";
    return { line, label: m[2] ?? "", len: m[0].length };
  }
  return null;
}

/** Reads `id` or `id[label]` at the start of `s`. */
function readNode(s: string): { id: string; label?: string; shape?: Shape; len: number } | null {
  const lead = /^\s*/.exec(s)![0].length;
  const m = ID.exec(s.slice(lead));
  if (!m) return null;
  let len = lead + m[0].length;
  const rest = s.slice(len);
  for (const [shape, re] of SHAPE_RE) {
    const sm = re.exec(rest);
    if (sm) return { id: m[0], label: unquote(sm[1]), shape, len: len + sm[0].length };
  }
  return { id: m[0], len };
}

/** Parses a flowchart, or returns null when it's not one the visual editor can handle. */
export function parseFlow(src: string): Flow | null {
  const lines = src.split("\n");
  let i = 0;
  while (i < lines.length && (!lines[i].trim() || lines[i].trim().startsWith("%%"))) i++;
  const head = /^\s*(flowchart|graph)(?:\s+(LR|TD|TB|RL|BT))?\s*;?\s*$/i.exec(lines[i] ?? "");
  if (!head) return null;
  const flow: Flow = {
    keyword: head[1].toLowerCase() as Flow["keyword"],
    dir: ((head[2] ?? "TD").toUpperCase().replace("TB", "TD")) as Dir,
    nodes: [],
    edges: [],
    extra: [],
  };
  const node = (id: string, label?: string, shape?: Shape) => {
    let n = flow.nodes.find((x) => x.id === id);
    if (!n) {
      n = { id, label: label ?? id, shape: shape ?? "rect" };
      flow.nodes.push(n);
    } else if (label !== undefined) {
      n.label = label;
      n.shape = shape ?? n.shape;
    }
    return n;
  };
  for (const raw of lines.slice(i + 1)) {
    for (const stmt of raw.split(";")) {
      const s = stmt.trim();
      if (!s) continue;
      if (/^(subgraph|end)\b/.test(s)) return null; // groups: code view only
      if (/^(%%|classDef|class|style|linkStyle|click|direction)\b/.test(s)) {
        flow.extra.push(s);
        continue;
      }
      // A chain: node (link node)*
      let rest = s;
      const first = readNode(rest);
      if (!first) {
        flow.extra.push(s);
        continue;
      }
      const chainNodes: { id: string; label?: string; shape?: Shape }[] = [first];
      const chainLinks: { line: Line; label: string }[] = [];
      rest = rest.slice(first.len);
      let ok = true;
      while (rest.trim()) {
        const link = readLink(rest);
        if (!link) {
          ok = false;
          break;
        }
        rest = rest.slice(link.len);
        const next = readNode(rest);
        if (!next) {
          ok = false;
          break;
        }
        rest = rest.slice(next.len);
        chainLinks.push(link);
        chainNodes.push(next);
      }
      if (!ok) {
        flow.extra.push(s);
        continue;
      }
      for (const n of chainNodes) node(n.id, n.label, n.shape);
      chainLinks.forEach((l, k) => flow.edges.push({ from: chainNodes[k].id, to: chainNodes[k + 1].id, label: unquote(l.label), line: l.line }));
    }
  }
  return flow;
}

function labelText(s: string): string {
  // Plain words stay plain; anything with brackets, quotes or pipes is quoted.
  return /[()[\]{}<>|"#;]/.test(s) ? `"${s.replace(/"/g, "#quot;")}"` : s;
}

export function serializeFlow(f: Flow): string {
  const out = [`${f.keyword} ${f.dir}`];
  for (const n of f.nodes) {
    const [, , open, close] = SHAPES.find((x) => x[0] === n.shape)!;
    out.push(`    ${n.id}${open}${labelText(n.label || " ")}${close}`);
  }
  for (const e of f.edges) {
    const op = LINES.find((x) => x[0] === e.line)![2];
    out.push(`    ${e.from} ${op}${e.label ? `|${labelText(e.label)}|` : ""} ${e.to}`);
  }
  for (const x of f.extra) out.push(`    ${x}`);
  return out.join("\n");
}

/** A node id not used yet: N1, N2… */
export function freshId(f: Flow): string {
  for (let n = f.nodes.length + 1; ; n++) {
    const id = `N${n}`;
    if (!f.nodes.some((x) => x.id === id)) return id;
  }
}

/** Starter diagrams for the template picker. */
export const TEMPLATES: [string, string][] = [
  ["Flowchart", "flowchart LR\n    A[Start] --> B{Decision}\n    B -->|Yes| C[Do it]\n    B -->|No| D[Skip]"],
  ["Sequence", "sequenceDiagram\n    participant A as Alice\n    participant B as Bob\n    A->>B: Hello Bob\n    B-->>A: Hi Alice"],
  ["Mind map", "mindmap\n  root((Idea))\n    Branch one\n      Detail\n    Branch two"],
  ["Timeline", "timeline\n    title Project\n    2026-10 : Plan\n    2026-11 : Build\n    2026-12 : Launch"],
  ["Gantt", "gantt\n    title Plan\n    dateFormat YYYY-MM-DD\n    section Work\n    Design :a1, 2026-10-20, 5d\n    Build  :after a1, 10d"],
  ["Pie chart", 'pie title Time spent\n    "Writing" : 50\n    "Reading" : 30\n    "Other" : 20'],
  ["State", "stateDiagram-v2\n    [*] --> Draft\n    Draft --> Review\n    Review --> Done\n    Done --> [*]"],
  ["Class", "classDiagram\n    class Note {\n      +title\n      +links()\n    }\n    Note <|-- DailyNote"],
  ["Entity relation", "erDiagram\n    PROJECT ||--o{ TASK : has\n    TASK }o--|| PERSON : \"assigned to\""],
];
