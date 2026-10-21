// Speed, safety, features, why it's different, and the call to action.
import React, { useLayoutEffect, useMemo, useRef } from "react";
import { Img, interpolate, staticFile, useCurrentFrame } from "remotion";
import { BODY, C, Caption, Check, DISPLAY, LockIcon, MONO, Scene, Window, clamp, ease, rise, rng, typed, useLayout, useSpring } from "./kit";
import T from "./timeline.json";

const S = T.scenes;
const dur = (k: keyof typeof S) => S[k][1] - S[k][0] + 12;

// ------------------------------------------------------------------ speed

const SP0 = S.speed[0];
const FILL = T.zip[0] - SP0; // the grid fills in real time: 0.45 s = 27 frames
const FILL_FRAMES = 27;
const PART_B = 300;

/** 10,000 dots, one per note, lighting up as the vault opens. */
const NoteGrid: React.FC<{ size: number; x: number; y: number; opacity: number }> = ({ size, x, y, opacity }) => {
  const f = useCurrentFrame();
  const ref = useRef<HTMLCanvasElement>(null);
  const order = useMemo(() => {
    const r = rng(42);
    return Array.from({ length: 10000 }, () => r());
  }, []);
  const p = interpolate(f, [FILL, FILL + FILL_FRAMES], [0, 1], clamp);
  const glow = interpolate(f, [FILL + FILL_FRAMES, FILL + FILL_FRAMES + 40], [1, 0], clamp);
  useLayoutEffect(() => {
    const c = ref.current;
    if (!c) return;
    const ctx = c.getContext("2d")!;
    ctx.clearRect(0, 0, size, size);
    const cell = size / 100;
    const d = cell * 0.62;
    for (let i = 0; i < 10000; i++) {
      const on = order[i] < p;
      const cx = (i % 100) * cell;
      const cy = Math.floor(i / 100) * cell;
      if (on) {
        const l = 45 + order[(i * 7) % 10000] * 25 + glow * 20;
        ctx.fillStyle = `hsl(193, 85%, ${l}%)`;
      } else ctx.fillStyle = "#2a2a27";
      ctx.fillRect(cx, cy, d, d);
    }
  }, [p, glow, size, order]);
  return <canvas ref={ref} width={size} height={size} style={{ position: "absolute", left: x, top: y, width: size, height: size, opacity }} />;
};

const RESULTS = [
  { t: "Ferry schedule", s: "Morning ferry leaves at 7:40 from the north terminal" },
  { t: "Trip plan", s: "Book the ferry to Halifax" },
  { t: "Packing list", s: "Snacks and a jacket for the ferry deck" },
  { t: "Halifax", s: "Ferry to Dartmouth runs every 15 minutes" },
];

const Stat: React.FC<{ at: number; value: number; unit: string; dec: number; label: string; w: number }> = ({ at, value, unit, dec, label, w }) => {
  const f = useCurrentFrame();
  const p = useSpring(at);
  const n = interpolate(f, [at, at + 40], [0, value], { ...clamp, easing: ease });
  return (
    <div style={{ width: w, padding: "40px 46px", borderRadius: 28, background: C.night2, border: `1.5px solid ${C.nightRule}`, ...rise(p, 50) }}>
      <div style={{ font: `600 120px/1 ${BODY}`, color: C.tealLight, fontVariantNumeric: "tabular-nums" }}>
        {n.toLocaleString("en-US", { minimumFractionDigits: dec, maximumFractionDigits: dec })}
        <span style={{ fontSize: 60, marginLeft: 12, color: C.nightSoft }}>{unit}</span>
      </div>
      <div style={{ font: `400 38px/1.3 ${BODY}`, color: C.nightInk, marginTop: 16 }}>{label}</div>
    </div>
  );
};

export const Speed: React.FC = () => {
  const { W, H, P } = useLayout();
  const f = useCurrentFrame();
  const g = f + SP0;
  const a = interpolate(f, [PART_B - 20, PART_B], [1, 0], clamp);
  const b = interpolate(f, [PART_B, PART_B + 20], [0, 1], clamp);
  const clock = interpolate(f, [FILL, FILL + FILL_FRAMES], [0, 0.45], clamp);
  const done = useSpring(FILL + FILL_FRAMES, 12);
  const q = typed(T.typing[5].text, T.typing[5].start, T.typing[5].rate, g);
  const gridSize = P ? 1200 : 1000;
  const sw = P ? 1300 : 1320;
  const statW = P ? 1300 : 880;
  return (
    <Scene bg={`radial-gradient(ellipse at 70% 30%, #1b2427 0%, ${C.night} 65%)`} dur={dur("speed")}>
      {/* Part A: open 10,000 notes */}
      <div style={{ position: "absolute", inset: 0, opacity: a }}>
        <Caption night x={P ? 100 : 160} y={P ? 170 : 300} w={P ? W - 200 : 900} align={P ? "center" : "left"} size={P ? 120 : 140} eyebrow="Speed" title={<>Fast. <i style={{ color: C.tealLight }}>Really</i> fast.</>} sub="Marrow is built in Rust. Watch it open 10,000 notes, in real time:" />
        <NoteGrid size={gridSize} x={P ? (W - gridSize) / 2 : 1400} y={P ? 760 : 220} opacity={useSpring(20)} />
        <div style={{ position: "absolute", left: P ? 0 : 160, right: P ? 0 : undefined, top: P ? 2010 : 900, textAlign: P ? "center" : "left" }}>
          <div style={{ font: `600 ${P ? 220 : 240}px/1 ${BODY}`, color: done > 0.5 ? C.tealLight : C.nightInk, fontVariantNumeric: "tabular-nums", transform: `scale(${1 + done * 0.06 - Math.max(0, done - 0.95) * 0})`, transformOrigin: P ? "center" : "left center" }}>
            {clock.toFixed(2)}
            <span style={{ fontSize: 110, color: C.nightSoft, marginLeft: 16 }}>s</span>
          </div>
          <div style={{ font: `400 46px ${BODY}`, color: C.nightSoft, marginTop: 10, ...rise(done, 20) }}>10,000 notes open and ready</div>
        </div>
      </div>
      {/* Part B: search and other timings */}
      <div style={{ position: "absolute", inset: 0, opacity: b }}>
        <Caption night at={PART_B + 4} x={P ? 100 : 160} y={P ? 170 : 110} w={P ? W - 200 : W - 320} align={P ? "center" : "left"} size={P ? 110 : 110} eyebrow="Speed" title="No waiting. Anywhere." />
        <Window night x={P ? (W - sw) / 2 : 160} y={P ? 620 : 420} w={sw} h={P ? 1060 : 900} title="Search all notes">
          <div style={{ padding: "44px 50px", width: "100%" }}>
            <div style={{ display: "flex", alignItems: "center", gap: 22, height: 100, padding: "0 30px", borderRadius: 18, background: "#0f0f0e", border: `2px solid ${q ? C.teal : C.nightRule}`, font: `400 50px ${BODY}`, color: C.nightInk }}>
              <svg width="44" height="44" viewBox="0 0 24 24" fill="none" stroke={C.nightSoft} strokeWidth={2.2}>
                <circle cx="11" cy="11" r="7" />
                <path d="M20 20l-4-4" strokeLinecap="round" />
              </svg>
              {q}
              <span style={{ width: 4, height: 54, background: C.tealLight, opacity: Math.floor(f / 18) % 2 ? 0.2 : 1 }} />
              {q && <span style={{ marginLeft: "auto", padding: "8px 22px", borderRadius: 30, background: "rgba(85,188,214,.14)", font: `600 32px ${BODY}`, color: C.tealLight }}>98 ms</span>}
            </div>
            {RESULTS.map((r, i) => {
              const p = q ? interpolate(g, [T.typing[5].start + i * 3, T.typing[5].start + i * 3 + 10], [0, 1], clamp) : 0;
              const word = q.toLowerCase();
              const k = r.s.toLowerCase().indexOf(word);
              return (
                <div key={r.t} style={{ marginTop: 26, padding: "26px 30px", borderRadius: 18, background: "#222220", ...rise(p, 20) }}>
                  <div style={{ font: `600 40px ${BODY}`, color: C.nightInk }}>{r.t}</div>
                  <div style={{ font: `400 34px/1.4 ${BODY}`, color: C.nightSoft, marginTop: 6 }}>
                    {k >= 0 && word ? (
                      <>
                        {r.s.slice(0, k)}
                        <mark style={{ background: "rgba(85,188,214,.3)", color: C.nightInk, borderRadius: 6, padding: "0 4px" }}>{r.s.slice(k, k + word.length)}</mark>
                        {r.s.slice(k + word.length)}
                      </>
                    ) : (
                      r.s
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </Window>
        <div style={{ position: "absolute", left: P ? (W - statW) / 2 : 1560, top: P ? 1740 : 420, display: "flex", flexDirection: "column", gap: P ? 30 : 34 }}>
          <Stat at={PART_B + 130} value={8.5} unit="ms" dec={1} label="to save a note" w={statW} />
          <Stat at={PART_B + 160} value={35} unit="ms" dec={0} label="to open a 2,000-row database" w={statW} />
          {!P && <Stat at={PART_B + 190} value={98} unit="ms" dec={0} label="to search every note" w={statW} />}
        </div>
        <div style={{ position: "absolute", left: 0, right: 0, bottom: P ? 70 : 54, textAlign: "center", font: `400 28px ${BODY}`, color: "#77746c", opacity: interpolate(f, [PART_B + 200, PART_B + 230], [0, 1], clamp) }}>
          Measured on a 10,000-note vault on a Windows PC.
        </div>
      </div>
    </Scene>
  );
};

// ------------------------------------------------------------------ safe

const SF0 = S.safe[0];
const LOCK_AT = 2540 - SF0;
const PART_SAFE_B = 330;

const CrashGrid: React.FC<{ x: number; y: number; label: string; bad: Set<number>; good?: boolean }> = ({ x, y, label, bad, good }) => {
  const f = useCurrentFrame();
  const n = Math.floor(interpolate(f, [60, 240], [0, 200], clamp));
  let damaged = 0;
  for (let i = 0; i < n; i++) if (bad.has(i)) damaged++;
  const cell = 40;
  const gap = 8;
  return (
    <div style={{ position: "absolute", left: x, top: y, ...rise(useSpring(20), 40) }}>
      <div style={{ font: `600 46px ${BODY}`, color: C.ink, marginBottom: 26, display: "flex", alignItems: "baseline", gap: 24 }}>
        {label}
        <span style={{ marginLeft: "auto", font: `700 46px ${BODY}`, color: good ? C.teal : C.red, fontVariantNumeric: "tabular-nums" }}>
          {damaged} damaged
        </span>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: `repeat(20, ${cell}px)`, gap }}>
        {Array.from({ length: 200 }, (_, i) => {
          const on = i < n;
          const isBad = on && bad.has(i);
          return <div key={i} style={{ width: cell, height: cell, borderRadius: 8, background: !on ? "#e6e3db" : isBad ? C.red : good ? C.teal : "#8fb9a4", transform: isBad && i === n - 1 ? "scale(1.25)" : undefined }} />;
        })}
      </div>
    </div>
  );
};

const SECRET = "Private journal\nStarted the new job today. Nervous, but excited about the team.";
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

export const Safe: React.FC = () => {
  const { W, P } = useLayout();
  const f = useCurrentFrame();
  const bad = useMemo(() => {
    const r = rng(9);
    const s = new Set<number>();
    while (s.size < 42) s.add(Math.floor(r() * 200));
    return s;
  }, []);
  const a = interpolate(f, [PART_SAFE_B - 20, PART_SAFE_B], [1, 0], clamp);
  const b = interpolate(f, [PART_SAFE_B, PART_SAFE_B + 20], [0, 1], clamp);
  const locked = interpolate(f, [LOCK_AT, LOCK_AT + 8], [1, 0], clamp);
  const scramble = interpolate(f, [LOCK_AT + 20, LOCK_AT + 90], [0, 1], clamp);
  const r = rng(3);
  const shown = SECRET.split("").map((ch, i) => {
    const rand = B64[Math.floor(r() * 64)];
    if (ch === "\n") return ch;
    return i / SECRET.length < scramble ? rand : ch;
  }).join("");
  const gw = 20 * 40 + 19 * 8;
  const cw = P ? 1240 : 1500;
  return (
    <Scene bg={C.paper} dur={dur("safe")}>
      <div style={{ position: "absolute", inset: 0, opacity: a }}>
        <Caption x={P ? 100 : 160} y={P ? 170 : 130} w={P ? W - 200 : W - 320} align="center" size={P ? 112 : 120} eyebrow="Safe" title="Your words survive crashes." sub="We force-quit Marrow 200 times in the middle of saving a note." />
        <CrashGrid x={P ? (W - gw) / 2 : 230} y={P ? 860 : 640} label="Ordinary save" bad={bad} />
        <CrashGrid x={P ? (W - gw) / 2 : W - 230 - gw} y={P ? 1560 : 640} label="Marrow" bad={new Set()} good />
        <div style={{ position: "absolute", left: 0, right: 0, top: P ? 2260 : 1230, textAlign: "center", font: `400 40px ${BODY}`, color: C.soft, ...rise(useSpring(250), 20) }}>
          Every save is all or nothing, and unsaved typing is kept as a draft.
        </div>
      </div>
      <div style={{ position: "absolute", inset: 0, opacity: b }}>
        <Caption at={PART_SAFE_B + 4} x={P ? 100 : 160} y={P ? 170 : 130} w={P ? W - 200 : W - 320} align="center" size={P ? 112 : 120} eyebrow="Private" title="Lock a folder. It's encrypted." sub="Without your password, other apps see only scrambled text." />
        <div
          style={{
            position: "absolute",
            left: (W - cw) / 2,
            top: P ? 900 : 620,
            width: cw,
            padding: "60px 70px",
            borderRadius: 28,
            background: "#fff",
            border: `1.5px solid ${C.rule}`,
            boxShadow: "0 40px 90px rgba(40,34,24,.14)",
            ...rise(useSpring(PART_SAFE_B + 20), 50),
          }}
        >
          <div style={{ position: "absolute", right: 50, top: -50, width: 120, height: 120, borderRadius: 60, background: C.sheet, border: `1.5px solid ${C.rule}`, display: "grid", placeItems: "center", transform: `scale(${1 + (f > LOCK_AT && f < LOCK_AT + 16 ? 0.15 : 0)})` }}>
            <LockIcon size={70} open={locked} />
          </div>
          <pre style={{ margin: 0, whiteSpace: "pre-wrap", wordBreak: "break-all", font: scramble > 0 ? `400 ${P ? 44 : 48}px/1.5 ${MONO}` : `400 ${P ? 48 : 52}px/1.5 ${BODY}`, color: scramble > 0 ? C.soft : C.ink }}>
            {shown}
          </pre>
        </div>
      </div>
    </Scene>
  );
};

// ------------------------------------------------------------------ features

// focus: the point to centre on (fractions of the screenshot); z: zoom beyond "cover".
const FEATURES = [
  { src: "08-graph.jpg", t: "Graph", s: "See how your notes connect.", fx: 0.4, fy: 0.55, zl: 1.25, zp: 1.0 },
  { src: "09-canvas.jpg", t: "Canvas", s: "Lay out ideas on an endless board.", fx: 0.45, fy: 0.5, zl: 1.3, zp: 1.0 },
  { src: "24-database-calendar.jpg", t: "Databases", s: "Turn a folder into a table, board or calendar.", fx: 0.62, fy: 0.42, zl: 1.45, zp: 1.05 },
  { src: "14-slides.jpg", t: "Slides", s: "Present any note as a slide deck.", fx: 0.5, fy: 0.5, zl: 1.2, zp: 1.0 },
  { src: "31-site-note.jpg", t: "Publish", s: "Share your notes as a website.", fx: 0.35, fy: 0.3, zl: 1.0, zp: 1.0 },
  { src: "29-focus-mode.jpg", t: "Make it yours", s: "Dark theme, focus mode and your own styles.", fx: 0.5, fy: 0.36, zl: 1.45, zp: 1.0 },
];
export const Features: React.FC = () => {
  const { W, H, P } = useLayout();
  const f = useCurrentFrame();
  const step = 80;
  const cw = P ? 1300 : 1720;
  const ch = P ? 1120 : (1720 * 792) / 1456;
  const cy = P ? 860 : 400;
  return (
    <Scene bg={`linear-gradient(180deg, ${C.paper} 0%, #e3eef0 100%)`} dur={dur("features")}>
      <div style={{ position: "absolute", inset: 0, perspective: 2600 }}>
        {FEATURES.map((ft, i) => {
          const t = (f - i * step) / 24;
          const inP = interpolate(t, [-1, 0], [0, 1], { ...clamp, easing: ease });
          const outP = i === FEATURES.length - 1 ? 0 : interpolate(f - (i + 1) * step, [-24, 0], [0, 1], { ...clamp, easing: ease });
          if (inP <= 0 || outP >= 1) return null;
          const x = (1 - inP) * 1500 - outP * 1500;
          const ry = (1 - inP) * -38 + outP * 38;
          const op = Math.min(inP, 1 - outP);
          const z = -(1 - inP) * 400 - outP * 400;
          return (
            <div key={ft.src} style={{ position: "absolute", left: (W - cw) / 2, top: cy, width: cw, height: ch, transform: `translateX(${x}px) translateZ(${z}px) rotateY(${ry}deg)`, opacity: op, borderRadius: 24, overflow: "hidden", boxShadow: "0 50px 110px rgba(30,40,40,.28)", border: `1.5px solid ${C.rule}` }}>
              {(() => {
                const z = (P ? ft.zp : ft.zl) * (1.02 + Math.max(0, f - i * step) * 0.0006);
                const sc = Math.max(cw / 1456, ch / 792) * z;
                const iw = 1456 * sc;
                const ih = 792 * sc;
                const left = Math.min(0, Math.max(cw - iw, cw / 2 - ft.fx * iw));
                const top = Math.min(0, Math.max(ch - ih, ch / 2 - ft.fy * ih));
                return <Img src={staticFile(ft.src)} style={{ position: "absolute", left, top, width: iw, height: ih }} />;
              })()}
            </div>
          );
        })}
      </div>
      {FEATURES.map((ft, i) => {
        const vis = interpolate(f - i * step, [-6, 10, step - 14, step], [0, 1, 1, i === FEATURES.length - 1 ? 1 : 0], clamp);
        return (
          <div key={ft.t} style={{ position: "absolute", left: 0, right: 0, top: P ? 400 : 120, textAlign: "center", opacity: vis, transform: `translateY(${(1 - vis) * 24}px)` }}>
            <div style={{ font: `500 ${P ? 130 : 120}px/1 ${DISPLAY}`, color: C.ink }}>{ft.t}</div>
            <div style={{ font: `400 ${P ? 48 : 50}px ${BODY}`, color: C.soft, marginTop: 20 }}>{ft.s}</div>
          </div>
        );
      })}
      <div style={{ position: "absolute", left: 0, right: 0, top: P ? cy + ch + 90 : H - 70, display: "flex", justifyContent: "center", gap: 18 }}>
        {FEATURES.map((ft, i) => (
          <div key={ft.t} style={{ width: f >= i * step && f < (i + 1) * step ? 54 : 16, height: 16, borderRadius: 8, background: f >= i * step ? C.teal : C.rule }} />
        ))}
      </div>
    </Scene>
  );
};

// ------------------------------------------------------------------ different

const POINTS = ["Plain Markdown files", "No account, no subscription", "Works offline", "Encrypted locked folders", "Free and open source", "Windows, macOS and Linux"];

export const Different: React.FC = () => {
  const { W, P } = useLayout();
  const f = useCurrentFrame();
  const base = T.pops[3] - S.different[0];
  return (
    <Scene bg={C.paper} dur={dur("different")}>
      <Caption x={100} y={P ? 260 : 130} w={W - 200} align="center" size={P ? 120 : 120} at={0} title={<>Why people switch to <i style={{ color: C.teal }}>Marrow</i></>} />
      <div style={{ position: "absolute", left: P ? 170 : 300, top: P ? 760 : 520, width: P ? W - 340 : W - 600, display: "grid", gridTemplateColumns: P ? "1fr" : "1fr 1fr", gap: P ? "70px 0" : "70px 120px" }}>
        {POINTS.map((t, i) => {
          const p = useSpring(base + i * 30, 12, 0.6);
          return (
            <div key={t} style={{ display: "flex", alignItems: "center", gap: 34, opacity: Math.min(1, p * 1.5), transform: `scale(${0.6 + 0.4 * p})`, transformOrigin: "left center" }}>
              <Check size={P ? 84 : 80} />
              <span style={{ font: `600 ${P ? 66 : 64}px ${BODY}`, color: C.ink }}>{t}</span>
            </div>
          );
        })}
      </div>
    </Scene>
  );
};

// ------------------------------------------------------------------ cta

export const Cta: React.FC = () => {
  const { W, H, P } = useLayout();
  const f = useCurrentFrame();
  const logo = useSpring(20, 12, 0.8);
  const name = useSpring(34);
  const tag = useSpring(70);
  const pills = useSpring(120);
  const url = useSpring(160);
  const end = interpolate(f, [S.cta[1] - S.cta[0] - 40, S.cta[1] - S.cta[0]], [1, 0], clamp);
  return (
    <Scene bg={`radial-gradient(ellipse at 50% 40%, #1d2629 0%, ${C.night} 70%)`} dur={dur("cta")}>
      <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: P ? 60 : 44, opacity: end, textAlign: "center", padding: "0 100px" }}>
        <Img src={staticFile("logo-mark.svg")} style={{ width: P ? 300 : 240, height: P ? 300 : 240, transform: `scale(${logo}) rotate(${(1 - logo) * -30}deg)`, filter: `drop-shadow(0 0 60px rgba(85,188,214,${0.5 * logo}))` }} />
        <div style={{ font: `500 ${P ? 220 : 220}px/1 ${DISPLAY}`, color: C.nightInk, ...rise(name, 40) }}>Marrow</div>
        <div style={{ font: `400 ${P ? 64 : 64}px/1.3 ${BODY}`, color: C.nightSoft, ...rise(tag, 30) }}>
          Your notes. Your files. <span style={{ color: C.tealLight }}>Your computer.</span>
        </div>
        <div style={{ display: "flex", flexDirection: P ? "column" : "row", gap: 24, ...rise(pills, 30) }}>
          {["Free and open source", "Windows · macOS · Linux"].map((t) => (
            <div key={t} style={{ padding: "22px 48px", borderRadius: 60, border: `2px solid ${C.teal}`, background: "rgba(11,120,150,.16)", font: `600 46px ${BODY}`, color: C.nightInk }}>
              {t}
            </div>
          ))}
        </div>
        <div style={{ font: `600 ${P ? 50 : 52}px ${BODY}`, color: C.tealLight, ...rise(url, 20) }}>github.com/mahbub-khan25/marrow</div>
      </div>
    </Scene>
  );
};
