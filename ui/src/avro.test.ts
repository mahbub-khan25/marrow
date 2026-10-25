import { describe, expect, it } from "vitest";
import { step, toBangla } from "./bangla";

describe("Bangla typing", () => {
  it("is Avro Phonetic", () => {
    expect(toBangla("ami")).toBe("আমি");
    expect(toBangla("bangladesh")).toBe("বাংলাদেশ");
    expect(toBangla("bondhu")).toBe("বন্ধু");
    expect(toBangla("ekushe")).toBe("একুশে");
    expect(toBangla("prem")).toBe("প্রেম");
    expect(toBangla("2026")).toBe("২০২৬");
  });
  it("re-reads the whole word on every key", () => {
    // "k" then "o" then "r"…: each step converts the letters typed so far.
    let w = step(undefined, 0, "", "k");
    expect(w.out).toBe("ক");
    for (const key of "orte") w = step(w, w.from + w.out.length, w.out, key);
    expect(w).toEqual({ from: 0, roman: "korte", out: "করতে" });
  });
  it("starts a new word when the cursor moved or the text changed", () => {
    const w = step(undefined, 5, "", "a");
    expect(step(w, 9, "", "m").from).toBe(9);
    expect(step(w, 5 + w.out.length, "x", "m").roman).toBe("m");
  });
});
