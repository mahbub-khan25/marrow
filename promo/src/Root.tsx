import React from "react";
import { Composition } from "remotion";
import { Promo, TOTAL } from "./Promo";
import { Promo2, TOTAL2 } from "./v2/Promo2";

export const Root: React.FC = () => (
  <>
    <Composition id="MarrowPromo" component={Promo} durationInFrames={TOTAL} fps={30} width={1920} height={1080} />
    <Composition id="MarrowPromo2Wide" component={Promo2} durationInFrames={TOTAL2} fps={60} width={2560} height={1440} />
    <Composition id="MarrowPromo2Tall" component={Promo2} durationInFrames={TOTAL2} fps={60} width={1440} height={2560} />
  </>
);
