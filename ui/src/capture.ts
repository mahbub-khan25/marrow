// Quick capture: a small box for jotting something down. A system-wide
// shortcut (Settings → Editor) or, on phones and tablets, the round + button
// brings it up. Text, voice notes and photos go to today's note, as a line in
// the log or as a to-do.

import { h, icon } from "./util";

export interface CaptureOptions {
  /** Saves the capture; `asTask` puts it under Tasks as `- [ ] …`. */
  save(text: string, asTask: boolean, files: File[]): Promise<void>;
  /** Called after saving or cancelling (e.g. to send Marrow back to the background). */
  done?(saved: boolean): void;
  /** The AI service that can turn voice notes into text, if any. */
  transcriber?(): Promise<string | null>;
  /** A voice note as text (null when it can't be done). */
  transcribe?(audio: File): Promise<string | null>;
}

let open = false;

const stamp = () => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}-${p(d.getMinutes())}-${p(d.getSeconds())}`;
};

/** The first recording format this webview supports (iOS records MP4/AAC, others WebM/Opus). */
function audioFormat(): { mime: string; ext: string } | null {
  if (typeof MediaRecorder === "undefined") return null;
  const options = [
    { mime: "audio/mp4", ext: "m4a" },
    { mime: "audio/webm;codecs=opus", ext: "webm" },
    { mime: "audio/webm", ext: "webm" },
    { mime: "audio/ogg;codecs=opus", ext: "ogg" },
  ];
  return options.find((o) => MediaRecorder.isTypeSupported(o.mime)) ?? null;
}

const clock = (ms: number) => {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

export function openCapture(o: CaptureOptions) {
  if (open) {
    document.querySelector<HTMLTextAreaElement>(".capture textarea")?.focus();
    return;
  }
  open = true;
  const touch = document.documentElement.hasAttribute("data-mobile");
  const area = h("textarea.capture-input", { placeholder: "What's on your mind?", rows: 3, spellcheck: true }) as HTMLTextAreaElement;
  const asTask = h("input", { type: "checkbox", id: "capture-task" }) as HTMLInputElement;
  try {
    asTask.checked = localStorage.getItem("marrow.capture.task") === "1";
  } catch {
    /* fine */
  }

  // Voice notes and photos waiting to be saved.
  const files: File[] = [];
  const chips = h("div.capture-files");
  const status = h("div.capture-status", { role: "status" });
  const renderChips = () => {
    chips.replaceChildren(
      ...files.map((f, i) =>
        h(
          "span.capture-chip",
          null,
          icon(f.type.startsWith("audio") ? "microphone" : "image"),
          h("span", null, f.type.startsWith("audio") ? "Voice note" : f.name),
          h(
            "button.icon-btn",
            {
              title: "Remove",
              "aria-label": "Remove",
              onclick: () => {
                files.splice(i, 1);
                renderChips();
              },
            },
            icon("x"),
          ),
        ),
      ),
    );
  };

  // ---- voice
  const micLabel = h("span", null, "Voice note");
  const mic = h("button.btn.btn-secondary.capture-mic", { type: "button" }, icon("microphone"), micLabel) as HTMLButtonElement;
  let recorder: MediaRecorder | null = null;
  let stream: MediaStream | null = null;
  let timer = 0;
  const stopStream = () => {
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
    window.clearInterval(timer);
  };
  mic.onclick = async () => {
    if (recorder) {
      recorder.stop();
      return;
    }
    const fmt = audioFormat();
    if (!fmt || !navigator.mediaDevices?.getUserMedia) {
      status.textContent = "Voice notes aren't available here.";
      return;
    }
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch {
      status.textContent = "Marrow can't use the microphone. Allow it in your system's privacy settings, then try again.";
      return;
    }
    const chunks: Blob[] = [];
    recorder = new MediaRecorder(stream, { mimeType: fmt.mime });
    recorder.ondataavailable = (e) => e.data.size && chunks.push(e.data);
    const started = Date.now();
    mic.classList.add("is-recording");
    micLabel.textContent = "Stop 0:00";
    timer = window.setInterval(() => (micLabel.textContent = `Stop ${clock(Date.now() - started)}`), 500);
    status.textContent = "Recording…";
    recorder.onstop = async () => {
      stopStream();
      recorder = null;
      mic.classList.remove("is-recording");
      micLabel.textContent = "Voice note";
      const audio = new File(chunks, `Voice note ${stamp()}.${fmt.ext}`, { type: fmt.mime.split(";")[0] });
      if (!audio.size) {
        status.textContent = "Nothing was recorded.";
        return;
      }
      files.push(audio);
      renderChips();
      // Into text, with the person's own AI service, when they have one that can.
      const who = await o.transcriber?.().catch(() => null);
      if (!who || !o.transcribe) {
        status.textContent = `Voice note kept (${clock(Date.now() - started)}). To also get it as text, add OpenAI or Groq in Settings → AI.`;
        return;
      }
      status.textContent = `Turning it into text with ${who}…`;
      try {
        const text = await o.transcribe(audio);
        if (text) {
          area.value = area.value.trim() ? `${area.value.trim()} ${text}` : text;
          status.textContent = `Transcribed by ${who}. The recording is kept too.`;
        } else status.textContent = "Voice note kept.";
      } catch (e) {
        status.textContent = `Voice note kept, but it couldn't be turned into text: ${e}`;
      }
    };
    recorder.start(1000);
  };

  // ---- photos
  const picker = h("input", { type: "file", accept: "image/*", multiple: true, hidden: true }) as HTMLInputElement;
  // On a phone, open the camera straight away.
  if (touch) picker.setAttribute("capture", "environment");
  picker.onchange = () => {
    for (const f of Array.from(picker.files ?? [])) {
      const ext = (/\.([a-z0-9]+)$/i.exec(f.name)?.[1] ?? f.type.split("/")[1] ?? "jpg").replace("jpeg", "jpg");
      // Cameras name every photo "image.jpg"; give it a date.
      files.push(/^(image|photo)\.\w+$/i.test(f.name) || !f.name ? new File([f], `Photo ${stamp()}.${ext}`, { type: f.type }) : f);
    }
    picker.value = "";
    renderChips();
  };
  const photo = h("button.btn.btn-secondary", { type: "button", onclick: () => picker.click() }, icon(touch ? "camera" : "image"), touch ? "Photo" : "Picture");

  const saveBtn = h("button.btn.btn-primary", { type: "button" }, "Save") as HTMLButtonElement;
  const card = h(
    "div.capture",
    { role: "dialog", "aria-label": "Quick capture" },
    h("div.capture-head", null, icon("lightning"), h("span", null, "Quick capture"), h("span.capture-where", null, "→ today's note")),
    area,
    chips,
    h("div.capture-media", null, mic, photo, picker),
    status,
    h(
      "div.capture-foot",
      null,
      h("label.capture-task", null, asTask, " As a to-do"),
      touch ? null : h("span.capture-hint", null, "Enter to save · Shift+Enter for a new line · Esc to cancel"),
      h("div.capture-acts", null, h("button.btn.btn-secondary", { type: "button", onclick: () => close(false) }, "Cancel"), saveBtn),
    ),
  );
  const wrap = h("div.backdrop.capture-wrap", null, card);
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
  area.focus();

  const close = (saved: boolean) => {
    if (!open) return;
    open = false;
    if (recorder) {
      recorder.onstop = null;
      recorder.stop();
    }
    stopStream();
    wrap.remove();
    document.documentElement.toggleAttribute("data-modal", false);
    o.done?.(saved);
  };
  const save = async () => {
    if (recorder) recorder.stop();
    const text = area.value.trim();
    if (!text && !files.length) return close(false);
    area.disabled = saveBtn.disabled = true;
    try {
      await o.save(text, asTask.checked, files);
      close(true);
    } catch (e) {
      area.disabled = saveBtn.disabled = false;
      status.textContent = String(e);
    }
  };
  saveBtn.onclick = () => void save();
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
  area.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close(false);
    } else if (e.key === "Enter" && !e.shiftKey && !touch) {
      e.preventDefault();
      void save();
    }
  });
}
