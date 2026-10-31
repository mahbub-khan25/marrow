// Drawings in Source mode: a line that is only a drawing's embed
// (`![[Drawing ….canvas|360]]`) shows the drawing itself, ready to draw on.
// Putting the cursor on that line (arrow keys) shows the embed text again.

import { RangeSetBuilder, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, WidgetType } from "@codemirror/view";

import { drawingInLine, lineWithHeight, type DrawingRef, type MountedDrawing } from "./drawing";

export type DrawingMount = (box: HTMLElement, ref: DrawingRef, setHeight: (h: number) => void) => MountedDrawing;

class DrawingWidget extends WidgetType {
  constructor(
    readonly ref: DrawingRef,
    readonly mount: DrawingMount,
  ) {
    super();
  }

  eq(o: DrawingWidget) {
    return o.ref.target === this.ref.target && o.ref.height === this.ref.height;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("div");
    const d = this.mount(box, this.ref, (height) => {
      // The widget's line may have moved: find it by where the box is now.
      const pos = view.posAtDOM(box);
      const line = view.state.doc.lineAt(pos);
      const next = lineWithHeight(line.text, height);
      if (next !== line.text) view.dispatch({ changes: { from: line.from, to: line.to, insert: next } });
    });
    (box as HTMLElement & { _drawing?: MountedDrawing })._drawing = d;
    return box;
  }

  destroy(dom: HTMLElement) {
    (dom as HTMLElement & { _drawing?: MountedDrawing })._drawing?.destroy();
  }

  get estimatedHeight() {
    return this.ref.height + 8;
  }

  ignoreEvent() {
    return true;
  }
}

function build(state: EditorState, mount: DrawingMount): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  const text = state.doc.toString();
  if (!/\.canvas/i.test(text)) return b.finish();
  const cursorLines = new Set(state.selection.ranges.map((r) => state.doc.lineAt(r.head).number));
  for (let n = 1; n <= state.doc.lines; n++) {
    const line = state.doc.line(n);
    if (cursorLines.has(n) || !line.text.includes(".canvas")) continue;
    const ref = drawingInLine(line.text);
    if (ref) b.add(line.from, line.to, Decoration.replace({ widget: new DrawingWidget(ref, mount), block: true }));
  }
  return b.finish();
}

/** Shows drawings in the editor; `mount` draws one into a box. */
export function drawingWidgets(mount: DrawingMount): Extension {
  return StateField.define<DecorationSet>({
    create: (state) => build(state, mount),
    update(deco, tr) {
      if (!tr.docChanged && !tr.selection) return deco;
      // Only rebuild when something drawing-related could have changed.
      const was = new Set(tr.startState.selection.ranges.map((r) => tr.startState.doc.lineAt(r.head).number));
      const now = new Set(tr.state.selection.ranges.map((r) => tr.state.doc.lineAt(r.head).number));
      const sameLines = !tr.docChanged && was.size === now.size && [...was].every((n) => now.has(n));
      return sameLines ? deco : build(tr.state, mount);
    },
    provide: (f) => EditorView.decorations.from(f),
  });
}
