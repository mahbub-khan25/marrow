// Hyperlume AI: offered only on a Hyperlume desktop, and made the default
// only when no AI is set up yet.
import { beforeEach, describe, expect, it, vi } from "vitest";

const state = { available: false, config: { providers: [], defaultId: null, embed: null } as Record<string, unknown>, saved: [] as unknown[], probes: 0 };

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "ai_hyperlume_available") {
      state.probes++;
      return state.available;
    }
    if (cmd === "ai_config") return state.config;
    if (cmd === "ai_save_provider") {
      state.saved.push(args?.provider);
      const p = { ...(args?.provider as object), id: "hyperlume-1" };
      state.config = { providers: [p], defaultId: "hyperlume-1", embed: null, hyperlumeOffered: true };
      return state.config;
    }
    throw new Error(`unexpected ${cmd}`);
  }),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { aiConfig, ensureHyperlume, hyperlumeAvailable, isLinuxDesktop, PRESETS, resetHyperlumeForTests, shouldAddHyperlume, visiblePresets } from "./ai";
import type { AiConfig, AiProvider } from "./api";

// [navigator.platform, navigator.userAgent] as each system's web view reports them.
const UA: Record<string, [string, string]> = {
  linux: ["Linux x86_64", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)"],
  windows: ["Win32", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Edg/130.0"],
  macos: ["MacIntel", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)"],
  android: ["Linux armv8l", "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Mobile"],
  ios: ["iPhone", "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko)"],
  ipad: ["MacIntel", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)"],
};

function setUA([platform, ua]: [string, string]) {
  Object.defineProperty(navigator, "platform", { value: platform, configurable: true });
  Object.defineProperty(navigator, "userAgent", { value: ua, configurable: true });
}

const provider = (preset: string): AiProvider => ({ id: `${preset}-1`, preset, name: preset, protocol: "openai", baseUrl: "", model: "m", hasKey: true });
const cfg = (providers: AiProvider[], extra: Partial<AiConfig> = {}): AiConfig => ({ providers, defaultId: providers[0]?.id ?? null, embed: null, ...extra });

beforeEach(() => {
  state.available = false;
  state.config = { providers: [], defaultId: null, embed: null };
  state.saved = [];
  state.probes = 0;
  resetHyperlumeForTests();
  setUA(UA.linux);
});

describe("the Hyperlume AI preset", () => {
  it("exists, needs no key and no address", () => {
    const p = PRESETS.find((x) => x.id === "hyperlume")!;
    expect(p.name).toBe("Hyperlume AI");
    expect(p.protocol).toBe("hyperlume");
    expect(p.needsKey).toBe(false);
    expect(p.baseUrl).toBe("");
  });

  it("is listed only when Hyperlume AI answers", () => {
    expect(visiblePresets(true).some((p) => p.id === "hyperlume")).toBe(true);
    expect(visiblePresets(false).some((p) => p.id === "hyperlume")).toBe(false);
    // Every other preset is there either way.
    expect(visiblePresets(false).length).toBe(PRESETS.length - 1);
  });

  it("is looked for only on a Linux desktop", () => {
    expect(isLinuxDesktop()).toBe(true);
    // Marrow's phone layout on Linux (for example a Linux phone) doesn't ask either.
    document.documentElement.setAttribute("data-mobile", "");
    expect(isLinuxDesktop()).toBe(false);
    document.documentElement.removeAttribute("data-mobile");
    for (const ua of [UA.windows, UA.macos, UA.android, UA.ios, UA.ipad]) {
      setUA(ua);
      expect(isLinuxDesktop(), ua[0]).toBe(false);
    }
  });

  it("is absent on Windows, macOS, Android and iOS, without even asking", async () => {
    state.available = true; // even if something answered
    for (const ua of [UA.windows, UA.macos, UA.android, UA.ios]) {
      resetHyperlumeForTests();
      setUA(ua);
      expect(await hyperlumeAvailable(), ua[0]).toBe(false);
    }
    expect(state.probes).toBe(0);
  });

  it("asks once on Linux and remembers the answer", async () => {
    state.available = true;
    expect(await hyperlumeAvailable()).toBe(true);
    expect(await hyperlumeAvailable()).toBe(true);
    expect(state.probes).toBe(1);
  });

  it("counts as unavailable when asking fails", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("no such command"));
    expect(await hyperlumeAvailable()).toBe(false);
  });
});

describe("choosing Hyperlume AI by default", () => {
  it("only when it's available and no AI is set up", () => {
    expect(shouldAddHyperlume(cfg([]), true)).toBe(true);
    expect(shouldAddHyperlume(cfg([]), false)).toBe(false);
  });

  it("never over an AI the person already chose", () => {
    expect(shouldAddHyperlume(cfg([provider("openai")]), true)).toBe(false);
    expect(shouldAddHyperlume(cfg([provider("ollama"), provider("anthropic")]), true)).toBe(false);
  });

  it("not again after the person removed it", () => {
    expect(shouldAddHyperlume(cfg([], { hyperlumeOffered: true }), true)).toBe(false);
  });

  it("adds it and makes it the default", async () => {
    state.available = true;
    expect(await ensureHyperlume()).toBe(true);
    expect(state.saved).toHaveLength(1);
    expect(state.saved[0]).toMatchObject({ preset: "hyperlume", protocol: "hyperlume", name: "Hyperlume AI", baseUrl: "", model: "", upstream: "" });
    expect(aiConfig().defaultId).toBe("hyperlume-1");
  });

  it("leaves a configured AI alone", async () => {
    state.available = true;
    state.config = cfg([provider("openai")]) as unknown as Record<string, unknown>;
    expect(await ensureHyperlume()).toBe(false);
    expect(state.saved).toHaveLength(0);
  });

  it("does nothing on other systems", async () => {
    state.available = true;
    setUA(UA.windows);
    expect(await ensureHyperlume()).toBe(false);
    expect(state.saved).toHaveLength(0);
  });
});
