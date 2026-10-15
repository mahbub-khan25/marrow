// The diagram editor: a Mermaid diagram's code on one side and a live
// preview on the other. Flowcharts can also be edited visually: click a box
// to rename it or change its shape, add boxes, connect them, label arrows.
// Everything is written back as ordinary Mermaid text.

import { loadMermaid } from "./codeblocks";
import { freshId, LINES, parseFlow, serializeFlow, SHAPES, TEMPLATES, type Dir, type Flow, type Line, type Shape } from "./flowchart";
import { h, icon, kbd } from "./util";

type Mode = "visual" | "code";

const DIRS: [Dir, string, string][] = [
  ["LR", "Left to right", "arrow-right"],
  ["TD", "Top to bottom", "arrow-down"],
  ["RL", "Right to left", "arrow-left"],
  ["BT", "Bottom to top", "arrow-up"],
];

let renders = 0;

/** Opens the editor on `source`; `onSave` gets the new text. */
export function openDiagramEditor(source: string, onSave: (text: string) => void) {
  let text = source.replace(/\s+$/, "");
  const original = text;
  let flow: Flow | null = parseFlow(text);
  let mode: Mode = flow ? "visual" : "code";
  let picked: string | null = null; // node id
  let pickedEdge: number | null = null;

  // ------------------------------------------------------------- chrome
  const preview = h("div.dge-preview");
  const error = h("div.dge-error", { hidden: true });
  const side = h("div.dge-side");
  const tabVisual = h("button", { role: "tab", onclick: () => setMode("visual") }, icon("cursor-click"), "Visual");
  const tabCode = h("button", { role: "tab", onclick: () => setMode("code") }, icon("code"), "Code");
  const template = h(
    "select.dge-template",
    { title: "Start from a template", "aria-label": "Start from a template" },
    h("option", { value: "" }, "Templates…"),
    ...TEMPLATES.map(([name], i) => h("option", { value: String(i) }, name)),
  ) as HTMLSelectElement;
  template.addEventListener("change", () => {
    const t = TEMPLATES[Number(template.value)];
    template.value = "";
    if (!t) return;
    setText(t[1]);
    mode = flow ? "visual" : "code";
    renderSide();
  });
  const save = h("button.btn.btn-primary", { onclick: () => finish(true) }, "Save");
  const card = h(
    "div.dge",
    { role: "dialog", "aria-label": "Edit diagram" },
    h(
      "div.dge-head",
      null,
      h("div.dge-title", null, icon("flow-arrow"), "Diagram"),
      h("div.dge-tabs", { role: "tablist" }, tabVisual, tabCode),
      template,
      h("div.grow"),
      h("span.dge-hint", null, `${kbd("Enter")} to save`),
      h("button.btn.btn-secondary", { onclick: () => finish(false) }, "Cancel"),
      save,
    ),
    h("div.dge-body", null, side, h("div.dge-stage", null, preview, error)),
  );
  const wrap = h("div.backdrop.dge-wrap", null, card);
  document.body.append(wrap);

  function finish(ok: boolean) {
    wrap.remove();
    document.removeEventListener("keydown", keys, true);
    if (ok && text !== original) onSave(text);
  }
  const keys = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && (e.key === "Enter" || e.key.toLowerCase() === "s")) {
      e.preventDefault();
      e.stopPropagation();
      finish(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      // Don't lose work to a stray Escape.
      if (text === original) finish(false);
      else {
        save.classList.remove("flash");
        void save.offsetWidth;
        save.classList.add("flash");
      }
    }
  };
  document.addEventListener("keydown", keys, true);

  // ----------------------------------------------------------- state

  function setText(next: string, fromFlow = false) {
    text = next;
    if (!fromFlow) flow = parseFlow(text);
    tabVisual.disabled = !flow;
    tabVisual.title = flow ? "Edit by clicking" : "Visual editing works for flowcharts. Use Code for other diagram types.";
    void draw();
  }

  function setFlow(f: Flow) {
    flow = f;
    setText(serializeFlow(f), true);
    renderSide();
  }

  function setMode(m: Mode) {
    if (m === "visual" && !flow) return;
    mode = m;
    renderSide();
  }

  // ---------------------------------------------------------- preview

  async function draw() {
    const mine = ++renders;
    const id = `dge-${mine}`;
    try {
      const mermaid = await loadMermaid();
      const { svg } = await mermaid.render(id, text || "flowchart LR");
      if (mine !== renders) return;
      preview.innerHTML = svg;
      error.hidden = true;
      markPicked();
    } catch (e) {
      document.getElementById(`d${id}`)?.remove();
      if (mine !== renders) return;
      error.hidden = false;
      error.textContent = String((e as Error)?.message ?? e).split("\n").slice(0, 3).join(" ");
    }
  }

  /** The flowchart node id of a drawn box. */
  const nodeIdOf = (g: Element): string | null => /-flowchart-(.+)-\d+$/.exec(g.id)?.[1] ?? null;

  function markPicked() {
    for (const g of preview.querySelectorAll("g.node")) g.classList.toggle("is-picked", mode === "visual" && nodeIdOf(g) === picked);
  }

  preview.addEventListener("click", (e) => {
    if (mode !== "visual" || !flow) return;
    const t = e.target as Element;
    const g = t.closest("g.node");
    if (g) {
      picked = nodeIdOf(g);
      pickedEdge = null;
      renderSide();
      side.querySelector<HTMLInputElement>(".dge-label")?.select();
      return;
    }
    // Arrows: their label or line carries "L_<from>_<to>_<n>".
    const ed = t.closest("[data-id^='L_'], [id*='-L_']");
    const key = ed?.getAttribute("data-id") ?? /-(L_.+)$/.exec(ed?.id ?? "")?.[1];
    if (key) {
      const k = flow.edges.findIndex((x) => key.startsWith(`L_${x.from}_${x.to}_`));
      if (k >= 0) {
        pickedEdge = k;
        picked = null;
        renderSide();
        side.querySelector<HTMLInputElement>(`.dge-edge[data-i="${k}"] input`)?.focus();
        return;
      }
    }
    picked = null;
    pickedEdge = null;
    renderSide();
  });

  // ------------------------------------------------------------- side

  function renderSide() {
    tabVisual.setAttribute("aria-selected", String(mode === "visual"));
    tabCode.setAttribute("aria-selected", String(mode === "code"));
    card.dataset.mode = mode;
    if (mode === "code") {
      const area = h("textarea.dge-code", { spellcheck: false, "aria-label": "Mermaid code" }) as HTMLTextAreaElement;
      area.value = text;
      let t: number | undefined;
      area.addEventListener("input", () => {
        window.clearTimeout(t);
        t = window.setTimeout(() => setText(area.value), 250);
      });
      area.addEventListener("keydown", (e) => {
        if (e.key === "Tab") {
          e.preventDefault();
          const s = area.selectionStart;
          area.setRangeText("    ", s, area.selectionEnd, "end");
          area.dispatchEvent(new Event("input"));
        }
      });
      side.replaceChildren(
        area,
        h(
          "div.dge-note",
          null,
          "Mermaid syntax. ",
          h("a", { href: "#", onclick: (e: Event) => (e.preventDefault(), void openDocs()) }, "Reference"),
        ),
      );
      markPicked();
      return;
    }
    const f = flow!;
    const parts: (HTMLElement | string)[] = [];

    // Direction
    parts.push(
      h(
        "div.dge-row",
        null,
        h("span.dge-label-sm", null, "Direction"),
        h(
          "div.seg.small",
          null,
          ...DIRS.map(([d, label, ic]) =>
            h("button", { title: label, "aria-pressed": String(f.dir === d), onclick: () => setFlow({ ...f, dir: d }) }, icon(ic)),
          ),
        ),
      ),
    );

    // Adding
    const add = (connect: boolean) => {
      const id = freshId(f);
      const nodes = [...f.nodes, { id, label: "New step", shape: "rect" as Shape }];
      const edges = connect && picked ? [...f.edges, { from: picked, to: id, label: "", line: "arrow" as Line }] : f.edges;
      picked = id;
      pickedEdge = null;
      setFlow({ ...f, nodes, edges });
      side.querySelector<HTMLInputElement>(".dge-label")?.select();
    };
    parts.push(
      h(
        "div.dge-row.dge-adds",
        null,
        h("button.btn.btn-secondary", { onclick: () => add(false) }, icon("plus"), "Box"),
        picked ? h("button.btn.btn-secondary", { onclick: () => add(true), title: "A new box with an arrow from the selected one" }, icon("arrow-elbow-down-right"), "Connected box") : "",
      ),
    );

    // The picked box
    const n = picked ? f.nodes.find((x) => x.id === picked) : null;
    if (n) {
      const label = h("input.input.dge-label", { value: n.label, spellcheck: true, "aria-label": "Box text" }) as HTMLInputElement;
      let lt: number | undefined;
      label.addEventListener("input", () => {
        window.clearTimeout(lt);
        lt = window.setTimeout(() => {
          n.label = label.value;
          flow = { ...f };
          setText(serializeFlow(f), true);
          // Keep the arrow list's names current without stealing focus.
          side.querySelectorAll<HTMLElement>(".dge-edge").forEach((row) => {
            const e = f.edges[Number(row.dataset.i)];
            const [a, b] = row.querySelectorAll<HTMLElement>(".dge-edge-ends span");
            if (e && a && b) {
              a.textContent = f.nodes.find((x) => x.id === e.from)?.label || e.from;
              b.textContent = f.nodes.find((x) => x.id === e.to)?.label || e.to;
            }
          });
        }, 200);
      });
      const others = f.nodes.filter((x) => x.id !== n.id);
      const connect = h(
        "select.dge-select",
        { "aria-label": "Draw an arrow to" },
        h("option", { value: "" }, "Arrow to…"),
        ...others.map((x) => h("option", { value: x.id }, x.label || x.id)),
      ) as HTMLSelectElement;
      connect.addEventListener("change", () => {
        if (!connect.value) return;
        setFlow({ ...f, edges: [...f.edges, { from: n.id, to: connect.value, label: "", line: "arrow" }] });
      });
      parts.push(
        h(
          "div.dge-card",
          null,
          h("div.dge-card-head", null, "Selected box"),
          label,
          h(
            "div.dge-shapes",
            null,
            ...SHAPES.map(([s, name]) =>
              h(
                "button",
                {
                  title: name,
                  "aria-pressed": String(n.shape === s),
                  onclick: () => {
                    n.shape = s;
                    setFlow({ ...f });
                  },
                },
                h(`span.dge-shape.is-${s}` as "span"),
              ),
            ),
          ),
          others.length ? connect : "",
          h(
            "button.dge-delete",
            {
              onclick: () => {
                picked = null;
                setFlow({ ...f, nodes: f.nodes.filter((x) => x.id !== n.id), edges: f.edges.filter((e) => e.from !== n.id && e.to !== n.id) });
              },
            },
            icon("trash"),
            "Delete box",
          ),
        ),
      );
    } else {
      parts.push(h("div.dge-note", null, "Click a box in the diagram to edit it, or an arrow to label it."));
    }

    // Arrows
    if (f.edges.length) {
      const name = (id: string) => f.nodes.find((x) => x.id === id)?.label || id;
      parts.push(h("div.dge-card-head", null, "Arrows"));
      parts.push(
        h(
          "div.dge-edges",
          null,
          ...f.edges.map((e, i) => {
            const lab = h("input.input", { value: e.label, placeholder: "Label", "aria-label": "Arrow label" }) as HTMLInputElement;
            let et: number | undefined;
            lab.addEventListener("input", () => {
              window.clearTimeout(et);
              et = window.setTimeout(() => {
                e.label = lab.value;
                setText(serializeFlow(f), true);
              }, 200);
            });
            const line = h("select.dge-select", { "aria-label": "Line style" }, ...LINES.map(([l, label]) => h("option", { value: l, selected: e.line === l }, label))) as HTMLSelectElement;
            line.addEventListener("change", () => {
              e.line = line.value as Line;
              setFlow({ ...f });
            });
            return h(
              `div.dge-edge${pickedEdge === i ? ".is-picked" : ""}` as "div",
              { dataset: { i: String(i) } },
              h("div.dge-edge-ends", null, h("span", null, name(e.from)), icon("arrow-right"), h("span", null, name(e.to))),
              h("div.dge-edge-ctl", null, lab, line, h("button.icon-btn", { title: "Delete arrow", onclick: () => setFlow({ ...f, edges: f.edges.filter((_, j) => j !== i) }) }, icon("x"))),
            );
          }),
        ),
      );
    }
    side.replaceChildren(...parts);
    markPicked();
  }

  async function openDocs() {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    void openUrl("https://mermaid.js.org/intro/syntax-reference.html").catch(() => {});
  }

  setText(text);
  renderSide();
  if (mode === "code") side.querySelector<HTMLTextAreaElement>("textarea")?.focus();
}
