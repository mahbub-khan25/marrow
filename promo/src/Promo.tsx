import React, { useEffect, useState } from "react";
import {
  AbsoluteFill,
  Easing,
  Img,
  Sequence,
  continueRender,
  delayRender,
  interpolate,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import "@fontsource/eb-garamond/500.css";
import "@fontsource/eb-garamond/600.css";
import "@fontsource/eb-garamond/500-italic.css";
import "@fontsource/source-sans-3/400.css";
import "@fontsource/source-sans-3/600.css";

// Marrow's own palette: warm paper, ink, teal links, magenta tags.
const C = {
  paper: "#f4f3ef",
  sheet: "#fbfaf7",
  ink: "#201f1c",
  soft: "#625f57",
  rule: "#dcd9d0",
  teal: "#0b7896",
  tealLight: "#55bcd6",
  tag: "#b3195f",
  night: "#131313",
  nightInk: "#ece9e2",
  nightSoft: "#a29e95",
};
const DISPLAY = '"EB Garamond", Georgia, serif';
const BODY = '"Source Sans 3", "Segoe UI", sans-serif';
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const ease = Easing.bezier(0.45, 0, 0.2, 1);

// ---------------------------------------------------------------- pieces

/** Fades a scene in over its first frames, so scenes cross-dissolve. */
const Scene: React.FC<{ bg: string; children: React.ReactNode; fadeOut?: boolean; dur?: number }> = ({ bg, children, fadeOut, dur = 0 }) => {
  const f = useCurrentFrame();
  const inO = interpolate(f, [0, 14], [0, 1], clamp);
  const outO = fadeOut ? interpolate(f, [dur - 20, dur], [1, 0], clamp) : 1;
  return <AbsoluteFill style={{ background: bg, opacity: inO * outO }}>{children}</AbsoluteFill>;
};

type Zoom = { from: number; to: number; ox: number; oy: number };

/** A screenshot in a soft window frame, with a slow push-in toward a point. */
const Shot: React.FC<{
  src: string;
  width: number;
  aspect?: number;
  left: number;
  top: number;
  zoom?: Zoom;
  dur: number;
  delay?: number;
  rotate?: number;
  dark?: boolean;
  fadeAt?: [number, number];
}> = ({ src, width, aspect = 1456 / 792, left, top, zoom = { from: 1, to: 1.06, ox: 50, oy: 50 }, dur, delay = 0, rotate = 0, dark, fadeAt }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const enter = spring({ frame: f - delay, fps, config: { damping: 200, mass: 0.9 } });
  const s = interpolate(f, [delay, dur], [zoom.from, zoom.to], { ...clamp, easing: ease });
  const vis = fadeAt ? interpolate(f, [fadeAt[0], fadeAt[1]], [1, 0], clamp) : 1;
  return (
    <div
      style={{
        position: "absolute",
        left,
        top,
        width,
        height: width / aspect,
        borderRadius: 16,
        overflow: "hidden",
        background: dark ? "#1b1b1a" : C.sheet,
        border: `1px solid ${dark ? "#2e2d2a" : C.rule}`,
        boxShadow: dark
          ? "0 30px 80px rgba(0,0,0,.55), 0 2px 6px rgba(0,0,0,.4)"
          : "0 30px 70px rgba(40,34,24,.16), 0 2px 6px rgba(40,34,24,.08)",
        opacity: enter * vis,
        transform: `translateY(${(1 - enter) * 70}px) rotate(${rotate * enter}deg)`,
      }}
    >
      <Img
        src={staticFile(src)}
        style={{ width: "100%", height: "100%", objectFit: "cover", transform: `scale(${s})`, transformOrigin: `${zoom.ox}% ${zoom.oy}%` }}
      />
    </div>
  );
};

/** Eyebrow, headline and a line of copy, rising in one after another. */
const Caption: React.FC<{
  eyebrow: string;
  title: React.ReactNode;
  sub: string;
  left: number;
  top: number;
  width: number;
  size?: number;
  align?: "left" | "center";
  night?: boolean;
  delay?: number;
}> = ({ eyebrow, title, sub, left, top, width, size = 92, align = "left", night, delay = 4 }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const rise = (d: number) => {
    const p = spring({ frame: f - delay - d, fps, config: { damping: 200 } });
    return { opacity: p, transform: `translateY(${(1 - p) * 34}px)` };
  };
  return (
    <div style={{ position: "absolute", left, top, width, textAlign: align, display: "flex", flexDirection: "column", gap: 22, alignItems: align === "center" ? "center" : "flex-start" }}>
      <div style={{ ...rise(0), font: `600 24px ${BODY}`, letterSpacing: "0.2em", textTransform: "uppercase", color: night ? C.tealLight : C.teal }}>{eyebrow}</div>
      <div style={{ ...rise(5), font: `600 ${size}px/1.04 ${DISPLAY}`, color: night ? C.nightInk : C.ink, letterSpacing: "-0.01em" }}>{title}</div>
      <div style={{ ...rise(11), font: `400 34px/1.38 ${BODY}`, color: night ? C.nightSoft : C.soft, maxWidth: width }}>{sub}</div>
    </div>
  );
};

const Mark: React.FC<{ size: number; color: string; draw?: number }> = ({ size, color, draw = 1 }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <path d="M5 7.5 11 5l5 3 5-3 6 2.5V26l-6-2.5-5 3-5-3L5 26z" fill={color} opacity={0.12 * draw} />
    <path
      d="M5 7.5 11 5l5 3 5-3 6 2.5V26l-6-2.5-5 3-5-3L5 26zM11 5v18.5M16 8v18.5M21 5v18.5"
      fill="none"
      stroke={color}
      strokeWidth={1.6}
      strokeLinejoin="round"
      strokeDasharray={160}
      strokeDashoffset={160 * (1 - draw)}
    />
  </svg>
);

const Em: React.FC<{ children: React.ReactNode; night?: boolean }> = ({ children, night }) => (
  <span style={{ fontStyle: "italic", fontWeight: 500, color: night ? C.tealLight : C.teal }}>{children}</span>
);

// ---------------------------------------------------------------- scenes

const Intro: React.FC<{ dur: number }> = ({ dur }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const draw = interpolate(f, [4, 46], [0, 1], { ...clamp, easing: ease });
  const word = spring({ frame: f - 26, fps, config: { damping: 200 } });
  const line = spring({ frame: f - 48, fps, config: { damping: 200 } });
  const chips = spring({ frame: f - 72, fps, config: { damping: 200 } });
  const drift = interpolate(f, [0, dur], [1, 1.04]);
  return (
    <Scene bg={C.paper}>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", transform: `scale(${drift})` }}>
        <div style={{ display: "flex", alignItems: "center", gap: 34 }}>
          <Mark size={150} color={C.ink} draw={draw} />
          <div style={{ font: `600 190px/1 ${DISPLAY}`, color: C.ink, opacity: word, transform: `translateX(${(1 - word) * -30}px)`, letterSpacing: "-0.015em" }}>Marrow</div>
        </div>
        <div style={{ marginTop: 34, font: `500 56px/1.2 ${DISPLAY}`, color: C.ink, opacity: line, transform: `translateY(${(1 - line) * 24}px)` }}>
          Notes that stay <Em>yours</Em>, and link themselves.
        </div>
        <div style={{ marginTop: 40, display: "flex", gap: 18, opacity: chips }}>
          {["Plain Markdown", "Local-first", "Works offline"].map((t) => (
            <div key={t} style={{ font: `600 26px ${BODY}`, color: C.soft, border: `1.5px solid ${C.rule}`, borderRadius: 99, padding: "10px 26px", background: C.sheet }}>{t}</div>
          ))}
        </div>
      </AbsoluteFill>
    </Scene>
  );
};

const Write: React.FC<{ dur: number }> = ({ dur }) => (
  <Scene bg={C.paper}>
    <Caption left={110} top={300} width={560} eyebrow="Write" title={<>Markdown,<br />without the <Em>marks</Em>.</>} sub="Notes read like a finished page. Type / for headings, callouts, tables, maths and diagrams." />
    <Sequence from={0} durationInFrames={92} layout="none">
      <Shot src="03-preview.jpg" width={1100} left={720} top={240} dur={92} zoom={{ from: 1, to: 1.42, ox: 44, oy: 42 }} fadeAt={[80, 92]} />
    </Sequence>
    <Sequence from={80} durationInFrames={90} layout="none">
      <Shot src="06-slash-menu.jpg" width={1100} left={720} top={240} dur={90} zoom={{ from: 1.35, to: 1.85, ox: 38, oy: 58 }} fadeAt={[78, 90]} />
    </Sequence>
    <Sequence from={158} durationInFrames={dur - 158} layout="none">
      <Shot src="07-diagram-editor.jpg" width={1100} left={720} top={240} dur={dur - 158} zoom={{ from: 1.6, to: 1.8, ox: 50, oy: 50 }} />
    </Sequence>
  </Scene>
);

const Link: React.FC<{ dur: number }> = ({ dur }) => (
  <Scene bg={C.paper}>
    <Sequence from={0} durationInFrames={100} layout="none">
      <Shot src="05-properties-links.jpg" width={1100} left={100} top={240} dur={100} zoom={{ from: 1.9, to: 2.3, ox: 44, oy: 24 }} fadeAt={[88, 100]} />
    </Sequence>
    <Sequence from={88} durationInFrames={dur - 88} layout="none">
      <Shot src="08-graph.jpg" width={1100} left={100} top={240} dur={dur - 88} zoom={{ from: 1.3, to: 1.7, ox: 60, oy: 56 }} />
    </Sequence>
    <Caption left={1270} top={310} width={560} eyebrow="Link" title={<>Link ideas.<br />Watch them <Em>connect</Em>.</>} sub="Type [[ to link a note. Backlinks, tags and the graph build themselves." />
  </Scene>
);

const Plan: React.FC<{ dur: number }> = ({ dur }) => (
  <Scene bg={C.paper}>
    <Caption left={260} top={70} width={1400} align="center" size={84} eyebrow="Plan" title={<>Any folder becomes a <Em>database</Em>.</>} sub="Table, board or calendar. Every row is a real note." />
    <Shot src="22-database-table.jpg" width={760} left={110} top={470} dur={dur} delay={14} rotate={-3} zoom={{ from: 2.5, to: 2.65, ox: 50, oy: 45 }} />
    <Shot src="24-database-calendar.jpg" width={760} left={1050} top={470} dur={dur} delay={30} rotate={3} zoom={{ from: 1.95, to: 2.1, ox: 50, oy: 60 }} />
    <Shot src="23-database-board.jpg" width={800} left={560} top={430} dur={dur} delay={22} zoom={{ from: 2.45, to: 2.6, ox: 50, oy: 44 }} />
  </Scene>
);

const Canvas: React.FC<{ dur: number }> = ({ dur }) => (
  <Scene bg={C.paper}>
    <Shot src="09-canvas.jpg" width={1300} left={520} top={180} dur={dur} zoom={{ from: 1.3, to: 1.6, ox: 62, oy: 52 }} />
    <Caption left={110} top={330} width={440} size={88} eyebrow="Canvas" title={<>Think on an endless <Em>board</Em>.</>} sub="Cards, shapes, arrows and ink, side by side with your notes." />
  </Scene>
);

const Present: React.FC<{ dur: number }> = ({ dur }) => {
  const f = useCurrentFrame();
  const s = interpolate(f, [0, dur], [1.16, 1.02], { ...clamp, easing: ease });
  return (
    <Scene bg="#f2f2f0">
      <AbsoluteFill style={{ overflow: "hidden" }}>
        <Img src={staticFile("14-slides.jpg")} style={{ width: "100%", height: "100%", objectFit: "cover", transform: `translateX(330px) scale(${s})`, transformOrigin: "50% 45%" }} />
      </AbsoluteFill>
      <AbsoluteFill style={{ background: "linear-gradient(90deg, rgba(242,242,240,.97) 0%, rgba(242,242,240,.9) 26%, rgba(242,242,240,0) 46%)" }} />
      <Caption left={110} top={330} width={560} eyebrow="Present" title={<>Any note,<br />now a <Em>slide deck</Em>.</>} sub="Full screen in one click. Arrow keys to move on." />
    </Scene>
  );
};

const Lock: React.FC<{ open: number }> = ({ open }) => (
  <svg width="180" height="200" viewBox="0 0 90 100">
    <path d={`M24 46 V30 a21 21 0 0 1 42 0 V${46 - open * 14}`} fill="none" stroke={C.tealLight} strokeWidth="7" strokeLinecap="round" transform={`translate(0 ${-open * 8})`} />
    <rect x="12" y="44" width="66" height="50" rx="10" fill={C.tealLight} />
    <circle cx="45" cy="66" r="6" fill={C.night} />
    <rect x="42" y="68" width="6" height="13" rx="3" fill={C.night} />
  </svg>
);

const Private: React.FC<{ dur: number }> = ({ dur }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const pop = spring({ frame: f - 6, fps, config: { damping: 12, mass: 0.7 } });
  const open = interpolate(f, [120, 140], [0, 1], { ...clamp, easing: ease });
  return (
    <Scene bg={C.night}>
      <div style={{ position: "absolute", left: 110, top: 150, transform: `scale(${pop})`, transformOrigin: "left bottom" }}>
        <Lock open={open} />
      </div>
      <Caption night left={110} top={400} width={620} size={86} delay={14} eyebrow="Locked notes" title={<>Private means <Em night>private</Em>.</>} sub="Lock a note with a password. On disk it's unreadable, even when synced or copied." />
      <Sequence from={0} durationInFrames={128} layout="none">
        <Shot dark src="12-unlock-dialog.jpg" aspect={950 / 766} width={880} left={880} top={150} dur={128} delay={10} zoom={{ from: 2.0, to: 2.3, ox: 57, oy: 50 }} fadeAt={[116, 128]} />
      </Sequence>
      <Sequence from={116} durationInFrames={dur - 116} layout="none">
        <Shot dark src="13-unlocked.jpg" width={1060} left={790} top={250} dur={dur - 116} zoom={{ from: 2.4, to: 2.6, ox: 44, oy: 18 }} />
      </Sequence>
    </Scene>
  );
};

const Publish: React.FC<{ dur: number }> = ({ dur }) => (
  <Scene bg={C.paper}>
    <Caption left={110} top={300} width={600} eyebrow="Publish" title={<>Your vault,<br />as a <Em>website</Em>.</>} sub="One page per note, with links, search and diagrams. Works on phones. Locked notes stay out." />
    <Sequence from={0} durationInFrames={84} layout="none">
      <Shot src="25-publish-dialog.jpg" width={1080} left={740} top={250} dur={84} zoom={{ from: 2.3, to: 2.5, ox: 50, oy: 50 }} fadeAt={[72, 84]} />
    </Sequence>
    <Sequence from={72} durationInFrames={84} layout="none">
      <Shot src="26-published.jpg" width={1080} left={740} top={250} dur={84} zoom={{ from: 2.5, to: 2.65, ox: 50, oy: 52 }} fadeAt={[72, 84]} />
    </Sequence>
    <Sequence from={144} durationInFrames={96} layout="none">
      <Shot src="31-site-note.jpg" width={1080} left={740} top={250} dur={96} zoom={{ from: 1.12, to: 1.3, ox: 45, oy: 25 }} fadeAt={[84, 96]} />
    </Sequence>
    <Sequence from={228} durationInFrames={dur - 228} layout="none">
      <Shot src="33-site-phone.jpg" width={1080} left={740} top={250} dur={dur - 228} zoom={{ from: 1.02, to: 1.1, ox: 50, oy: 45 }} />
    </Sequence>
  </Scene>
);

const Yours: React.FC<{ dur: number }> = ({ dur }) => (
  <Scene bg={C.paper}>
    <Caption left={260} top={70} width={1400} align="center" size={84} eyebrow="Make it yours" title={<>Light or dark. Plugins. <Em>Your own AI</Em>.</>} sub="Plugins run sealed off from your files. AI only sees what you ask about." />
    <Shot src="28-dark-theme.jpg" width={560} left={110} top={500} dur={dur} delay={10} dark zoom={{ from: 1.6, to: 1.75, ox: 45, oy: 36 }} />
    <Shot src="18-settings-customize.jpg" width={560} left={680} top={500} dur={dur} delay={20} zoom={{ from: 2.6, to: 2.75, ox: 50, oy: 54 }} />
    <Shot src="16-settings-ai.jpg" width={560} left={1250} top={500} dur={dur} delay={30} zoom={{ from: 2.6, to: 2.75, ox: 50, oy: 46 }} />
  </Scene>
);

const FEATURES = ["Wikilinks", "Backlinks", "Graph", "Canvas", "Databases", "Tasks", "Daily notes", "Diagrams", "Maths", "Slides", "Locked notes", "Publish", "Bring-your-own AI", "Plugins", "Version history", "Obsidian-compatible"];

const Outro: React.FC<{ dur: number }> = ({ dur }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const a = spring({ frame: f - 4, fps, config: { damping: 200 } });
  const b = spring({ frame: f - 22, fps, config: { damping: 200 } });
  return (
    <Scene bg={C.paper} fadeOut dur={dur}>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", gap: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 26, opacity: a, transform: `scale(${0.94 + a * 0.06})` }}>
          <Mark size={110} color={C.ink} />
          <div style={{ font: `600 140px/1 ${DISPLAY}`, color: C.ink }}>Marrow</div>
        </div>
        <div style={{ marginTop: 26, font: `500 50px/1.2 ${DISPLAY}`, color: C.ink, opacity: b }}>
          Your notes. Plain Markdown. <Em>Yours.</Em>
        </div>
        <div style={{ marginTop: 56, width: 1500, display: "flex", flexWrap: "wrap", justifyContent: "center", gap: 14 }}>
          {FEATURES.map((t, i) => {
            const p = spring({ frame: f - 34 - i * 3, fps, config: { damping: 200 } });
            const isTag = i % 5 === 2;
            return (
              <div key={t} style={{ opacity: p, transform: `translateY(${(1 - p) * 16}px)`, font: `600 25px ${BODY}`, color: isTag ? C.tag : C.soft, background: C.sheet, border: `1.5px solid ${C.rule}`, borderRadius: 99, padding: "9px 22px" }}>
                {t}
              </div>
            );
          })}
        </div>
      </AbsoluteFill>
    </Scene>
  );
};

// ---------------------------------------------------------------- timeline

const OVERLAP = 12;
const SCENES: [React.FC<{ dur: number }>, number][] = [
  [Intro, 150],
  [Write, 240],
  [Link, 200],
  [Plan, 190],
  [Canvas, 160],
  [Present, 160],
  [Private, 240],
  [Publish, 300],
  [Yours, 190],
  [Outro, 200],
];
export const TOTAL = SCENES.reduce((n, [, d]) => n + d, 0) - OVERLAP * (SCENES.length - 1);

export const Promo: React.FC = () => {
  const [handle] = useState(() => delayRender("fonts"));
  useEffect(() => {
    Promise.all([
      document.fonts.load(`600 96px "EB Garamond"`),
      document.fonts.load(`italic 500 96px "EB Garamond"`),
      document.fonts.load(`500 56px "EB Garamond"`),
      document.fonts.load(`400 34px "Source Sans 3"`),
      document.fonts.load(`600 24px "Source Sans 3"`),
    ]).then(() => continueRender(handle), () => continueRender(handle));
  }, [handle]);
  let at = 0;
  return (
    <AbsoluteFill style={{ background: C.paper }}>
      {SCENES.map(([S, d], i) => {
        const from = at;
        at += d - OVERLAP;
        return (
          <Sequence key={i} from={from} durationInFrames={d}>
            <S dur={d} />
          </Sequence>
        );
      })}
    </AbsoluteFill>
  );
};
