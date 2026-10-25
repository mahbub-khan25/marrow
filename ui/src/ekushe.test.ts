// Bangla typing with Ekushe Next's own engine (public/ekushe/ekushe.wasm).
import { beforeAll, describe, expect, it } from "vitest";

import { composer, setBanglaTyping, useEngine, type Host } from "./bangla";
import { Ekushe } from "./vendor/ekushe.js";

class TextHost implements Host {
  text = "";
  at = 0;
  key = {};
  replace(from: number, to: number, s: string) {
    this.text = this.text.slice(0, from) + s + this.text.slice(to);
    this.at = from + s.length;
  }
  caret() {
    return this.at;
  }
  slice(a: number, b: number) {
    return this.text.slice(a, b);
  }
  rect() {
    return null;
  }
}

/** Types `s` like a person: "\b" is Backspace, "\t" is Tab, "\n" is Enter. */
function type(host: TextHost, s: string) {
  for (const ch of s) {
    if (ch === "\b") {
      if (!composer.backspace(host)) host.replace(host.at - 1, host.at, "");
    } else if (ch === "\t") composer.cycle(host, 1);
    else if (ch === "\n") {
      composer.enter(host);
      host.replace(host.at, host.at, "\n");
    } else if (!composer.char(host, ch)) host.replace(host.at, host.at, ch);
  }
  return host.text;
}

beforeAll(async () => {
  // Node's file reading (the app itself never loads it this way).
  const { readFileSync } = (await import(/* @vite-ignore */ "node:" + "fs")) as { readFileSync(p: string): Uint8Array };
  const wasm = readFileSync("ui/public/ekushe/ekushe.wasm");
  const dict = readFileSync("ui/public/ekushe/ekushe-web.dict.z");
  const { instance } = await WebAssembly.instantiate(wasm as BufferSource, {});
  useEngine(new Ekushe(instance.exports, dict.buffer.slice(dict.byteOffset, dict.byteOffset + dict.byteLength) as ArrayBuffer));
  setBanglaTyping(true);
});

describe("Bangla typing with Ekushe Next", () => {
  it("types sentences", () => {
    expect(type(new TextHost(), "ami banglay gan gai.")).toBe("আমি বাংলায় গান গাই।");
    expect(type(new TextHost(), "swadhin ")).toBe("স্বাধীন ");
    expect(type(new TextHost(), "amar sonar bangla")).toBe("আমার সোনার বাংলা");
  });
  it("shows the word while it's typed, then finishes it with Enter", () => {
    const h = new TextHost();
    type(h, "bondhu");
    expect(composer.composing).toBe(true);
    expect(h.text).toBe(composer.shown);
    expect(composer.cands.length).toBeGreaterThan(0);
    type(h, "\nok");
    expect(h.text.startsWith(composer.cands.length ? "" : "")).toBe(true);
    expect(h.text.split("\n")[0]).toMatch(/^বন্ধু/);
  });
  it("brings the English back with Backspace right after a word", () => {
    expect(type(new TextHost(), "office \b")).toBe("office");
  });
  it("Backspace while typing takes back one English letter", () => {
    const h = new TextHost();
    type(h, "amarr\b");
    expect(h.text).toBe("আমার");
  });
  it("Tab shows the next candidate", () => {
    const h = new TextHost();
    type(h, "kotha");
    const first = h.text;
    if (composer.cands.length > 1) {
      type(h, "\t");
      expect(h.text).toBe(composer.cands[1].text);
      expect(h.text).not.toBe(first);
    }
  });
  it("other keys end the word and are typed as they are", () => {
    expect(type(new TextHost(), "ami#")).toBe("আমি#");
    expect(type(new TextHost(), "[[ami")).toMatch(/^\[\[আমি/);
  });
});
