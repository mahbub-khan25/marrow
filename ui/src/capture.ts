// Quick capture: a small box for jotting something down. A system-wide
// shortcut (Settings → Editor) brings it up from any app; what you type is
// added to today's note, as a line in the log or as a to-do.

import { h, icon } from "./util";

export interface CaptureOptions {
  /** Saves the text; `asTask` puts it under Tasks as `- [ ] …`. */
  save(text: string, asTask: boolean): Promise<void>;
  /** Called after saving or cancelling (e.g. to send Marrow back to the background). */
  done?(saved: boolean): void;
}

let open = false;

export function openCapture(o: CaptureOptions) {
  if (open) {
    document.querySelector<HTMLTextAreaElement>(".capture textarea")?.focus();
    return;
  }
  open = true;
  const area = h("textarea.capture-input", { placeholder: "What's on your mind?", rows: 3, spellcheck: true }) as HTMLTextAreaElement;
  const asTask = h("input", { type: "checkbox", id: "capture-task" }) as HTMLInputElement;
  try {
    asTask.checked = localStorage.getItem("marrow.capture.task") === "1";
  } catch {
    /* fine */
  }
  const card = h(
    "div.capture",
    { role: "dialog", "aria-label": "Quick capture" },
    h("div.capture-head", null, icon("lightning"), h("span", null, "Quick capture"), h("span.capture-where", null, "→ today's note")),
    area,
    h(
      "div.capture-foot",
      null,
      h("label.capture-task", null, asTask, " As a to-do"),
      h("span.capture-hint", null, "Enter to save · Shift+Enter for a new line · Esc to cancel"),
    ),
  );
  const wrap = h("div.backdrop.capture-wrap", null, card);
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
  area.focus();

  const close = (saved: boolean) => {
    if (!open) return;
    open = false;
    wrap.remove();
    document.documentElement.toggleAttribute("data-modal", false);
    o.done?.(saved);
  };
  wrap.addEventListener("mousedown", (e) => {
    if (e.target === wrap) close(false);
  });
  asTask.addEventListener("change", () => {
    try {
      localStorage.setItem("marrow.capture.task", asTask.checked ? "1" : "0");
    } catch {
      /* fine */
    }
  });
  area.addEventListener("keydown", async (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close(false);
    } else if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      const text = area.value.trim();
      if (!text) return close(false);
      area.disabled = true;
      try {
        await o.save(text, asTask.checked);
        close(true);
      } catch {
        area.disabled = false;
      }
    }
  });
}
