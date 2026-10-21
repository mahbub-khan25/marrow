import React from "react";
import { AbsoluteFill, Audio, Sequence, staticFile } from "remotion";
import { Files, Hook, Write } from "./scenes1";
import { Cta, Different, Features, Safe, Speed } from "./scenes2";
import T from "./timeline.json";

export const TOTAL2 = T.total;
const S = T.scenes;
const parts: [keyof typeof S, React.FC][] = [
  ["hook", Hook],
  ["files", Files],
  ["write", Write],
  ["speed", Speed],
  ["safe", Safe],
  ["features", Features],
  ["different", Different],
  ["cta", Cta],
];

export const Promo2: React.FC = () => (
  <AbsoluteFill style={{ background: "#111110" }}>
    {parts.map(([k, Comp]) => (
      <Sequence key={k} from={S[k][0]} durationInFrames={S[k][1] - S[k][0] + 12} name={k}>
        <Comp />
      </Sequence>
    ))}
    <Audio src={staticFile("promo2-audio.wav")} />
  </AbsoluteFill>
);
