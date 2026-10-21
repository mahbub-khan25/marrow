// Hook, plain files and live writing.
import React from "react";
import { Img, interpolate, staticFile, useCurrentFrame } from "remotion";
import { BODY, C, Caption, DISPLAY, DocIcon, Scene, Window, clamp, ease, rise, rng, typed, useLayout, useSpring } from "./kit";
import T from "./timeline.json";

const S = T.scenes;
const dur = (k: keyof typeof S) => S[k][1] - S[k][0] + 12;

// ------------------------------------------------------------------ hook

const NOTES = [
  { t: "Sign in to see your notes", s: "Your session has expired" },
  { t: "Syncing… 47%", s: "Please keep the app open" },
  { t: "Export is a Premium feature", s: "Upgrade to download your own notes" },
  { t: "You're offline", s: "Your notes aren't available right now" },
  { t: "Something went wrong", s: "Your last changes may not have been saved" },
  { t: "Loading workspace…", s: "This is taking longer than usual" },
];

const Notice: React.FC<{ i: number; at: number; x: number; y: number; w: number; rot: number }> = ({ i, at, x, y, w, rot }) => {
  const f = useCurrentFrame();
  const p = useSpring(at, 14, 0.7);
  const fall = interpolate(f, [186 + i * 3, 236 + i * 3], [0, 1], { ...clamp, easing: ease });
  const n = NOTES[i];
  const warn = i === 4;
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: y,
        width: w,
        padding: "34px 40px",
        borderRadius: 26,
        background: "rgba(38,37,34,.94)",
        border: `1.5px solid ${warn ? "rgba(200,70,61,.6)" : "#3a3935"}`,
        boxShadow: "0 30px 80px rgba(0,0,0,.5)",
        display: "flex",
        gap: 30,
        alignItems: "center",
        opacity: Math.min(1, p * 1.4) * (1 - fall),
        transform: `translateY(${fall * 520}px) scale(${0.82 + 0.18 * p}) rotate(${rot + fall * (i % 2 ? 14 : -12)}deg)`,
        filter: `blur(${fall * 8}px)`,
      }}
    >
      <div style={{ width: 70, height: 70, flex: "none", borderRadius: 18, background: warn ? C.red : "#4a4842", display: "grid", placeItems: "center", font: `700 40px ${BODY}`, color: "#fff" }}>
        {warn ? "!" : i === 1 || i === 5 ? "↻" : i === 3 ? "⌁" : "•"}
      </div>
      <div>
        <div style={{ font: `600 44px/1.2 ${BODY}`, color: C.nightInk }}>{n.t}</div>
        <div style={{ font: `400 32px/1.3 ${BODY}`, color: C.nightSoft, marginTop: 6 }}>{n.s}</div>
      </div>
    </div>
  );
};

export const Hook: React.FC = () => {
  const { W, H, P } = useLayout();
  const f = useCurrentFrame();
  const r = rng(7);
  const w = P ? 1180 : 1040;
  const spots = P
    ? NOTES.map((_, i) => ({ x: (W - w) / 2 + (i % 2 ? 60 : -60), y: 380 + i * 300 }))
    : NOTES.map((_, i) => ({ x: [260, 1290, 520, 1360, 200, 1120][i], y: [170, 260, 560, 700, 930, 1040][i] }));
  const words = ["Your", "notes", "deserve", "better."];
  const meet = useSpring(300, 16);
  const shake = f >= 150 && f < 186 ? Math.sin(f * 2.1) * 6 : 0;
  return (
    <Scene bg={`radial-gradient(ellipse at 50% 40%, #1f1e1b 0%, ${C.night} 70%)`} dur={dur("hook")}>
      <div style={{ position: "absolute", inset: 0, transform: `translateX(${shake}px)` }}>
        {NOTES.map((_, i) => (
          <Notice key={i} i={i} at={T.dings[i]} x={spots[i].x} y={spots[i].y} w={w} rot={(r() - 0.5) * 7} />
        ))}
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: P ? H * 0.36 : H * 0.3, textAlign: "center", display: "flex", justifyContent: "center", flexWrap: "wrap", gap: "0 0.28em", padding: "0 80px", font: `500 ${P ? 170 : 190}px/1.05 ${DISPLAY}`, color: C.nightInk }}>
        {words.map((wd, i) => {
          const p = interpolate(f, [214 + i * 9, 234 + i * 9], [0, 1], { ...clamp, easing: ease });
          return (
            <span key={wd} style={{ display: "inline-block", opacity: p, transform: `translateY(${(1 - p) * 60}px)`, filter: `blur(${(1 - p) * 10}px)`, fontStyle: i === 3 ? "italic" : "normal", color: i === 3 ? C.tealLight : undefined }}>
              {wd}
            </span>
          );
        })}
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: P ? H * 0.36 + 420 : H * 0.3 + 300, display: "flex", justifyContent: "center", alignItems: "center", gap: 34, ...rise(meet, 30) }}>
        <Img src={staticFile("logo-mark.svg")} style={{ width: 110, height: 110, filter: `drop-shadow(0 0 ${30 * meet}px rgba(85,188,214,.6))` }} />
        <div style={{ font: `500 96px ${DISPLAY}`, color: C.nightInk }}>Meet Marrow.</div>
      </div>
    </Scene>
  );
};

// ------------------------------------------------------------------ files

const FILES = ["Trip plan", "Halifax", "Ferry schedule", "Packing list", "Reading notes", "Ideas"];

export const Files: React.FC = () => {
  const { W, P } = useLayout();
  const f = useCurrentFrame();
  const pw = P ? 1200 : 900;
  const ph = P ? 640 : 720;
  const A = P ? { x: (W - pw) / 2, y: 820 } : { x: 300, y: 560 };
  const B = P ? { x: (W - pw) / 2, y: 1660 } : { x: W - 300 - pw, y: 560 };
  const rowH = 82;
  const top = 64 + 34;
  const ext = useSpring(250);
  const chips = useSpring(280);
  return (
    <Scene bg={C.paper} dur={dur("files")}>
      <Caption
        x={P ? 100 : 300}
        y={P ? 170 : 130}
        w={P ? W - 200 : W - 600}
        align="center"
        size={P ? 110 : 116}
        eyebrow="The difference"
        title={<>Every note is a plain file. <i style={{ color: C.teal }}>Yours.</i></>}
        sub="Marrow saves Markdown files in a folder on your computer. No account, no lock-in."
      />
      <Window x={A.x} y={A.y} w={pw} h={ph} title="Marrow" style={rise(useSpring(20), 60)}>
        <div style={{ padding: "34px 40px", width: "100%" }}>
          {FILES.map((n, i) => (
            <div key={n} style={{ height: rowH, display: "flex", alignItems: "center", gap: 22, font: `600 40px ${BODY}`, color: i === 0 ? C.teal : C.ink, borderRadius: 14, padding: "0 20px", background: i === 0 ? "rgba(11,120,150,.08)" : "transparent" }}>
              <DocIcon size={36} color={i === 0 ? C.teal : C.faint} />
              {n}
            </div>
          ))}
        </div>
      </Window>
      <Window x={B.x} y={B.y} w={pw} h={ph} title="Notes  ›  your folder" style={rise(useSpring(34), 60)}>
        <div style={{ padding: "34px 40px", width: "100%" }}>
          {FILES.map((n, i) => {
            const p = interpolate(f, [90 + i * 16, 130 + i * 16], [0, 1], { ...clamp, easing: ease });
            return (
              <div key={n} style={{ height: rowH, display: "flex", alignItems: "center", gap: 22, font: `400 40px ${BODY}`, color: C.ink, padding: "0 20px", opacity: p }}>
                <DocIcon size={36} color={C.soft} />
                <span>
                  {n}
                  <span style={{ color: C.teal, fontWeight: 700, opacity: ext }}>.md</span>
                </span>
                <span style={{ marginLeft: "auto", color: C.faint, fontSize: 32 }}>{[2, 4, 1, 1, 6, 3][i]} KB</span>
              </div>
            );
          })}
        </div>
      </Window>
      {/* Notes flying from the app into the folder. */}
      {FILES.map((n, i) => {
        const p = interpolate(f, [70 + i * 16, 130 + i * 16], [0, 1], { ...clamp, easing: ease });
        if (p <= 0 || p >= 1) return null;
        const sx = A.x + 60;
        const sy = A.y + top + i * rowH;
        const ex = B.x + 60;
        const ey = B.y + top + i * rowH;
        const x = sx + (ex - sx) * p;
        const y = sy + (ey - sy) * p - Math.sin(p * Math.PI) * (P ? 0 : 160);
        const xo = P ? Math.sin(p * Math.PI) * 220 : 0;
        return (
          <div key={n} style={{ position: "absolute", left: x + xo, top: y, height: rowH - 8, padding: "0 26px", display: "flex", alignItems: "center", gap: 18, borderRadius: 16, background: C.sheet, border: `1.5px solid ${C.teal}`, boxShadow: "0 18px 40px rgba(11,120,150,.25)", font: `600 38px ${BODY}`, color: C.teal, transform: `scale(${1 + Math.sin(p * Math.PI) * 0.08})` }}>
            <DocIcon size={32} color={C.teal} />
            {n}
          </div>
        );
      })}
      <div style={{ position: "absolute", left: 0, right: 0, top: P ? 2380 : 1330, display: "flex", justifyContent: "center", gap: 24, ...rise(chips, 30) }}>
        {["Open in any app", "Back up anywhere", "Keep them forever"].map((t) => (
          <div key={t} style={{ padding: "16px 34px", borderRadius: 40, border: `1.5px solid ${C.rule}`, background: C.sheet, font: `600 ${P ? 34 : 36}px ${BODY}`, color: C.soft }}>
            {t}
          </div>
        ))}
      </div>
    </Scene>
  );
};

// ------------------------------------------------------------------ write

const W0 = S.write[0];
const [L1, L2, L3, L4, L5] = T.typing;
const ENTER = 940 - W0;
const SAVED = 1040 - W0;
const PANEL = 400;

const Caret: React.FC<{ on: boolean }> = ({ on }) => {
  const f = useCurrentFrame();
  return <span style={{ display: "inline-block", width: 4, height: "1.05em", verticalAlign: "-0.15em", marginLeft: 2, background: C.teal, opacity: on && Math.floor(f / 18) % 2 === 0 ? 1 : on ? 0.15 : 0 }} />;
};

export const Write: React.FC = () => {
  const { W, P } = useLayout();
  const f = useCurrentFrame();
  const g = f + W0; // absolute frame for the shared timeline
  const h1 = typed(L1.text, L1.start, L1.rate, g);
  const body = typed(L2.text, L2.start, L2.rate, g);
  const open = typed(L3.text, L3.start, L3.rate, g);
  const q = typed(L4.text, L4.start, L4.rate, g);
  const linked = f >= ENTER;
  const tag = typed(L5.text, L5.start, L5.rate, g);
  const menu = interpolate(f, [158, 166, ENTER - 2, ENTER + 6], [0, 1, 1, 0], clamp);
  const options = ["Halifax", "Halifax food guide", "Ferry schedule", "Packing list"].filter((o) => o.toLowerCase().includes(q.toLowerCase()));
  const saved = useSpring(SAVED, 12);
  const savedOut = interpolate(f, [SAVED + 120, SAVED + 140], [1, 0], clamp);
  const panel = useSpring(PANEL);
  const caretAt = f < L2.start - W0 ? 1 : 2;

  const ww = P ? 1300 : 1500;
  const wh = P ? 1560 : 1100;
  const wx = P ? (W - ww) / 2 : 960;
  const wy = P ? 880 : 200;
  const textSize = P ? 50 : 52;

  const graph = interpolate(f, [PANEL + 40, PANEL + 160], [0, 1], { ...clamp, easing: ease });
  const nodes = [
    { x: 0.5, y: 0.5, n: "Halifax", main: true },
    { x: 0.18, y: 0.25, n: "Trip plan" },
    { x: 0.82, y: 0.22, n: "Ferry schedule" },
    { x: 0.2, y: 0.8, n: "Food guide" },
    { x: 0.8, y: 0.78, n: "Packing list" },
  ];
  const gw = P ? 1180 : 460;
  const gh = P ? 420 : 520;

  return (
    <Scene bg={`linear-gradient(160deg, ${C.paper} 0%, #e9f1f2 100%)`} dur={dur("write")}>
      <Caption
        x={P ? 100 : 150}
        y={P ? 170 : 430}
        w={P ? W - 200 : 720}
        align={P ? "center" : "left"}
        size={P ? 104 : 108}
        eyebrow="Writing"
        title={<>Just start typing.</>}
        sub={<>Type <b style={{ color: C.teal }}>[[</b> to link a note, <b style={{ color: C.tag }}>#</b> to tag it. Saving is automatic.</>}
        out={PANEL - 10}
      />
      <Caption
        x={P ? 100 : 150}
        y={P ? 170 : 430}
        w={P ? W - 200 : 720}
        align={P ? "center" : "left"}
        size={P ? 104 : 108}
        at={PANEL + 10}
        eyebrow="Connected"
        title={<>Links work both ways.</>}
        sub="Every note shows what links to it, and the graph shows how it all fits together."
      />
      <Window x={wx} y={wy} w={ww} h={wh} title="Marrow — Trip plan" style={rise(useSpring(4), 80)}>
        {!P && (
          <div style={{ width: 300, flex: "none", borderRight: `1.5px solid ${C.rule}`, padding: "30px 20px", background: "#f6f5f1" }}>
            {["Trip plan", "Halifax", "Ferry schedule", "Packing list", "Halifax food guide"].map((n, i) => (
              <div key={n} style={{ display: "flex", alignItems: "center", gap: 14, height: 62, padding: "0 14px", borderRadius: 12, font: `600 28px ${BODY}`, color: i === 0 ? C.teal : C.soft, background: i === 0 ? "rgba(11,120,150,.08)" : "transparent" }}>
                <DocIcon size={26} color={i === 0 ? C.teal : C.faint} />
                {n}
              </div>
            ))}
          </div>
        )}
        <div style={{ flex: 1, display: "flex", flexDirection: P ? "column" : "row", minWidth: 0 }}>
          <div style={{ flex: 1, position: "relative", padding: P ? "70px 80px" : "80px 90px", minWidth: 0 }}>
            <div style={{ font: `600 ${textSize * 1.55}px/1.2 ${DISPLAY}`, color: C.ink, minHeight: textSize * 1.9 }}>
              {h1.startsWith("#") && <span style={{ color: C.faint, fontWeight: 500 }}>#</span>}
              {h1.slice(1)}
              <Caret on={caretAt === 1} />
            </div>
            <div style={{ font: `400 ${textSize}px/1.6 ${BODY}`, color: C.ink, marginTop: 30, position: "relative" }}>
              {body}
              {!linked && (
                <>
                  <span style={{ color: C.faint }}>{open}</span>
                  <span style={{ color: C.teal }}>{q}</span>
                </>
              )}
              {linked && (
                <span style={{ color: C.teal, fontWeight: 600, borderBottom: `3px solid rgba(11,120,150,.35)`, background: `rgba(11,120,150,${0.18 * interpolate(f, [ENTER, ENTER + 30], [1, 0], clamp)})`, borderRadius: 6 }}>
                  Halifax
                </span>
              )}
              {tag && (
                <span>
                  {" "}
                  <span style={{ color: C.tag, fontWeight: 600, background: "rgba(179,25,95,.08)", borderRadius: 10, padding: "0 8px" }}>{tag.trim()}</span>
                </span>
              )}
              <Caret on={caretAt === 2 && f < SAVED + 60} />
              {/* [[ autocomplete */}
              <div
                style={{
                  position: "absolute",
                  left: P ? 120 : 420,
                  top: textSize * 1.75,
                  width: 620,
                  borderRadius: 18,
                  background: "#fff",
                  border: `1.5px solid ${C.rule}`,
                  boxShadow: "0 30px 70px rgba(40,34,24,.2)",
                  padding: 12,
                  opacity: menu,
                  transform: `translateY(${(1 - menu) * -16}px)`,
                }}
              >
                {options.map((o, i) => (
                  <div key={o} style={{ display: "flex", alignItems: "center", gap: 16, height: 74, padding: "0 20px", borderRadius: 12, font: `600 34px ${BODY}`, color: i === 0 ? "#fff" : C.ink, background: i === 0 ? C.teal : "transparent" }}>
                    <DocIcon size={28} color={i === 0 ? "#fff" : C.faint} />
                    {o}
                  </div>
                ))}
              </div>
            </div>
            <div style={{ position: "absolute", right: 50, bottom: 40, display: "flex", alignItems: "center", gap: 14, padding: "14px 28px", borderRadius: 40, background: "rgba(11,120,150,.1)", font: `600 32px ${BODY}`, color: C.teal, opacity: saved * savedOut, transform: `scale(${0.7 + 0.3 * saved})` }}>
              <span style={{ width: 16, height: 16, borderRadius: 8, background: "#3aa36a" }} />
              Saved · 8.5 ms
            </div>
          </div>
          {/* Inspector: linked mentions and a local graph. */}
          <div
            style={{
              flex: "none",
              width: P ? "auto" : 520 * panel,
              height: P ? 760 * panel : "auto",
              overflow: "hidden",
              borderLeft: P ? "none" : `1.5px solid ${C.rule}`,
              borderTop: P ? `1.5px solid ${C.rule}` : "none",
              background: "#f6f5f1",
            }}
          >
            <div style={{ width: P ? "100%" : 520, padding: "40px 30px", boxSizing: "border-box" }}>
              <div style={{ font: `700 24px ${BODY}`, letterSpacing: "0.16em", textTransform: "uppercase", color: C.faint }}>Linked mentions · Halifax</div>
              <div style={{ marginTop: 18, padding: "22px 24px", borderRadius: 16, background: "#fff", border: `1.5px solid ${C.rule}`, ...rise(useSpring(PANEL + 20), 20) }}>
                <div style={{ font: `600 30px ${BODY}`, color: C.ink }}>Trip plan</div>
                <div style={{ font: `400 27px/1.4 ${BODY}`, color: C.soft, marginTop: 6 }}>
                  Book the ferry to <span style={{ color: C.teal, fontWeight: 600 }}>Halifax</span>
                </div>
              </div>
              <svg width={gw} height={gh} style={{ marginTop: 24, display: "block" }}>
                {nodes.slice(1).map((n, i) => {
                  const p = Math.min(1, Math.max(0, graph * 1.4 - i * 0.12));
                  const x1 = nodes[0].x * gw;
                  const y1 = nodes[0].y * gh;
                  const x2 = x1 + (n.x * gw - x1) * p;
                  const y2 = y1 + (n.y * gh - y1) * p;
                  return <line key={n.n} x1={x1} y1={y1} x2={x2} y2={y2} stroke={C.teal} strokeOpacity={0.45} strokeWidth={3} />;
                })}
                {nodes.map((n, i) => {
                  const p = i === 0 ? Math.min(1, graph * 3) : Math.min(1, Math.max(0, graph * 1.4 - i * 0.12) * 1.2);
                  return (
                    <g key={n.n} opacity={p} transform={`translate(${n.x * gw} ${n.y * gh}) scale(${0.5 + 0.5 * p})`}>
                      <circle r={n.main ? 22 : 15} fill={n.main ? C.teal : "#fff"} stroke={C.teal} strokeWidth={4} />
                      <text y={n.main ? 58 : 48} textAnchor="middle" style={{ font: `600 ${n.main ? 28 : 24}px ${BODY}`, fill: n.main ? C.ink : C.soft }}>
                        {n.n}
                      </text>
                    </g>
                  );
                })}
              </svg>
            </div>
          </div>
        </div>
      </Window>
    </Scene>
  );
};
