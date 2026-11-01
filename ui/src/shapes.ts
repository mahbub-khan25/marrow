// The shapes drawings and canvases can hold. One list for every app: the
// phones read the same file through the engine (marrow-ffi shape_geometry).
// Each outline is in a 100 × 100 box, stretched to the card.

import data from "../../crates/marrow-core/data/shapes.json";

export interface ShapeDef {
  kind: string;
  name: string;
  /** The outline: SVG path data with M, L, C and Z only. */
  d: string;
  /** Lines drawn on top, not filled (the top of a cylinder). */
  extra?: string;
  /** Where words go: x0, y0, x1, y1 in the same box. */
  label: [number, number, number, number];
}

export const SHAPES: ShapeDef[] = (data.shapes as unknown as ShapeDef[]).map((s) => ({ ...s, label: s.label as ShapeDef["label"] }));

export const shapeDef = (kind: string | undefined): ShapeDef | undefined => SHAPES.find((s) => s.kind === kind);

/** The same path, stretched from the 100 × 100 box to a w × h box at (x, y). */
export function placePath(d: string, x: number, y: number, w: number, h: number): string {
  let i = 0;
  return d.replace(/-?\d+(?:\.\d+)?/g, (n) => {
    const v = Number(n);
    const out = i % 2 === 0 ? x + (v * w) / 100 : y + (v * h) / 100;
    i++;
    return String(Math.round(out * 10) / 10);
  });
}

/** Fill styles: see-through outline, light tint, or solid. */
export const FILLS: [string, string, string][] = [
  ["none", "No fill (see-through)", "selection-slash"],
  ["", "Light fill", "selection-background"],
  ["solid", "Solid fill", "square"],
];
