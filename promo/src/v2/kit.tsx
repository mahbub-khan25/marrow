// Shared look and helpers for the second promo.
import React from "react";
import { AbsoluteFill, Easing, interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import "@fontsource/eb-garamond/500.css";
import "@fontsource/eb-garamond/600.css";
import "@fontsource/eb-garamond/500-italic.css";
import "@fontsource/source-sans-3/400.css";
import "@fontsource/source-sans-3/600.css";
import "@fontsource/source-sans-3/700.css";
import "@fontsource/jetbrains-mono/400.css";

export const C = {
  paper: "#f4f3ef",
  sheet: "#fbfaf7",
  ink: "#201f1c",
  soft: "#625f57",
  faint: "#9a968c",
  rule: "#dcd9d0",
  teal: "#0b7896",
  tealLight: "#55bcd6",
  tag: "#b3195f",
  red: "#c8463d",
  night: "#111110",
  night2: "#1b1b19",
  nightRule: "#2e2d2a",
  nightInk: "#ece9e2",
  nightSoft: "#a29e95",
};
export const DISPLAY = '"EB Garamond", Georgia, serif';
export const BODY = '"Source Sans 3", "Segoe UI", sans-serif';
export const MONO = '"JetBrains Mono", Consolas, monospace';
export const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
export const ease = Easing.bezier(0.45, 0, 0.2, 1);

/** Canvas size and orientation: designs are drawn at 2560×1440 or 1440×2560. */
export function useLayout() {
  const { width, height } = useVideoConfig();
  return { W: width, H: height, P: height > width };
}

/** 0 → 1 spring starting at `at`. */
export function useSpring(at: number, damping = 200, mass = 1) {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  return spring({ frame: f - at, fps, config: { damping, mass } });
}

export function rise(p: number, dist = 40): React.CSSProperties {
  return { opacity: p, transform: `translateY(${(1 - p) * dist}px)` };
}

/** How much of `text` has been typed at frame f (one character every `rate` frames). */
export function typed(text: string, start: number, rate: number, f: number) {
  if (f < start) return "";
  return text.slice(0, Math.min(text.length, Math.floor((f - start) / rate) + 1));
}

export function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** A scene: fades in over its first frames and out over its last. */
export const Scene: React.FC<{ bg: string; dur: number; children: React.ReactNode; fade?: number }> = ({ bg, dur, children, fade = 12 }) => {
  const f = useCurrentFrame();
  const o = interpolate(f, [0, fade, dur - fade, dur], [0, 1, 1, 0], clamp);
  return <AbsoluteFill style={{ background: bg, opacity: o, overflow: "hidden" }}>{children}</AbsoluteFill>;
};

/** Eyebrow, headline and sub-line rising one after another. */
export const Caption: React.FC<{
  eyebrow?: string;
  title: React.ReactNode;
  sub?: React.ReactNode;
  at?: number;
  out?: number;
  x: number;
  y: number;
  w: number;
  size?: number;
  align?: "left" | "center";
  night?: boolean;
}> = ({ eyebrow, title, sub, at = 6, out, x, y, w, size = 120, align = "left", night }) => {
  const f = useCurrentFrame();
  const a = useSpring(at);
  const b = useSpring(at + 6);
  const c = useSpring(at + 12);
  const gone = out === undefined ? 1 : interpolate(f, [out, out + 14], [1, 0], clamp);
  return (
    <div style={{ position: "absolute", left: x, top: y, width: w, textAlign: align, opacity: gone, display: "flex", flexDirection: "column", gap: 28, alignItems: align === "center" ? "center" : "flex-start" }}>
      {eyebrow && (
        <div style={{ ...rise(a), font: `700 30px ${BODY}`, letterSpacing: "0.22em", textTransform: "uppercase", color: night ? C.tealLight : C.teal }}>{eyebrow}</div>
      )}
      <div style={{ ...rise(b, 50), font: `500 ${size}px/1.04 ${DISPLAY}`, letterSpacing: "-0.01em", color: night ? C.nightInk : C.ink }}>{title}</div>
      {sub && <div style={{ ...rise(c), font: `400 ${Math.round(size * 0.4)}px/1.4 ${BODY}`, color: night ? C.nightSoft : C.soft, maxWidth: w }}>{sub}</div>}
    </div>
  );
};

/** An app window frame. */
export const Window: React.FC<{
  x: number;
  y: number;
  w: number;
  h: number;
  title?: string;
  night?: boolean;
  style?: React.CSSProperties;
  children: React.ReactNode;
}> = ({ x, y, w, h, title = "Marrow", night, style, children }) => (
  <div
    style={{
      position: "absolute",
      left: x,
      top: y,
      width: w,
      height: h,
      borderRadius: 22,
      overflow: "hidden",
      background: night ? C.night2 : C.sheet,
      border: `1.5px solid ${night ? C.nightRule : C.rule}`,
      boxShadow: night ? "0 40px 100px rgba(0,0,0,.55)" : "0 40px 90px rgba(40,34,24,.16), 0 3px 8px rgba(40,34,24,.08)",
      display: "flex",
      flexDirection: "column",
      ...style,
    }}
  >
    <div style={{ height: 64, flex: "none", display: "flex", alignItems: "center", gap: 14, padding: "0 26px", borderBottom: `1.5px solid ${night ? C.nightRule : C.rule}`, background: night ? "#181816" : "#f1efe9" }}>
      {["#e0675c", "#e3b343", "#5fb760"].map((c) => (
        <div key={c} style={{ width: 18, height: 18, borderRadius: 9, background: c, opacity: 0.85 }} />
      ))}
      <div style={{ marginLeft: 18, font: `600 26px ${BODY}`, color: night ? C.nightSoft : C.soft }}>{title}</div>
    </div>
    <div style={{ flex: 1, position: "relative", display: "flex", minHeight: 0 }}>{children}</div>
  </div>
);

export const DocIcon: React.FC<{ size?: number; color?: string }> = ({ size = 30, color = C.faint }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={1.8} strokeLinejoin="round">
    <path d="M6 2.8h8l4.2 4.2V21.2H6z" />
    <path d="M14 2.8V7h4.2M8.8 12h6.4M8.8 15.6h6.4" />
  </svg>
);

export const Check: React.FC<{ size?: number; color?: string }> = ({ size = 56, color = C.teal }) => (
  <svg width={size} height={size} viewBox="0 0 24 24">
    <circle cx="12" cy="12" r="11" fill={color} />
    <path d="M7 12.4l3.2 3.2L17.2 8.6" stroke="#fff" strokeWidth={2.4} fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

export const LockIcon: React.FC<{ size?: number; open: number; color?: string }> = ({ size = 80, open, color = C.teal }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={2} strokeLinecap="round">
    <rect x="4.5" y="10.5" width="15" height="10.5" rx="2.2" fill={color} stroke="none" />
    <path d={`M8 10.5V7.5a4 4 0 0 1 8 0${open > 0.5 ? "" : "V10.5"}`} transform={`translate(${open * 5} ${-open * 1.5})`} />
    <circle cx="12" cy="15.7" r="1.6" fill="#fff" stroke="none" />
  </svg>
);
