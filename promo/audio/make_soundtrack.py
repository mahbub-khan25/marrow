"""Builds the promo soundtrack: an original ambient piano-and-pad bed,
with the voiceover clips placed at each scene and the music ducked under them.

    python audio/make_soundtrack.py

Writes public/music.wav (music only) and public/soundtrack.wav (the mix).
"""
import json
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, resample_poly, sosfilt, fftconvolve

SR = 48000
ROOT = Path(__file__).resolve().parent.parent
PUB = ROOT / "public"
FPS = 30
OVERLAP = 12
# Scene lengths in frames; keep in step with SCENES in src/Promo.tsx.
SCENES = [("intro", 150), ("write", 240), ("link", 200), ("plan", 190), ("canvas", 160),
          ("present", 160), ("private", 240), ("publish", 300), ("yours", 190), ("outro", 200)]
TOTAL = (sum(d for _, d in SCENES) - OVERLAP * (len(SCENES) - 1)) / FPS
VO_DELAY = 0.45  # seconds after a scene starts

rng = np.random.default_rng(7)


def hz(midi):
    return 440.0 * 2 ** ((midi - 69) / 12)


def env(n, a, r):
    e = np.ones(n)
    a, r = int(a * SR), int(r * SR)
    e[:a] = np.linspace(0, 1, a)
    e[-r:] *= np.linspace(1, 0, r)
    return e


def pad(midis, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    out = np.zeros(n)
    for m in midis:
        f = hz(m)
        for det in (-0.12, 0.0, 0.12):  # three slightly detuned voices
            ph = rng.uniform(0, 2 * np.pi)
            w = 2 * np.pi * f * (1 + det / 100) * t + ph
            out += np.sin(w) + 0.25 * np.sin(2 * w) + 0.08 * np.sin(3 * w)
    out *= env(n, 1.2, 1.4) / (len(midis) * 3)
    lfo = 0.85 + 0.15 * np.sin(2 * np.pi * 0.18 * t)
    return out * lfo


def pluck(m, dur=2.2, vel=1.0):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = hz(m)
    tone = np.sin(2 * np.pi * f * t) + 0.35 * np.sin(4 * np.pi * f * t) * np.exp(-t * 6) + 0.12 * np.sin(6 * np.pi * f * t) * np.exp(-t * 9)
    a = int(0.004 * SR)
    e = np.exp(-t * 2.6)
    e[:a] *= np.linspace(0, 1, a)
    return tone * e * vel


def bass(m, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    return (np.sin(2 * np.pi * hz(m) * t) + 0.2 * np.sin(4 * np.pi * hz(m) * t)) * env(n, 0.05, 0.8)


def place(buf, x, at):
    i = int(at * SR)
    j = min(len(buf), i + len(x))
    if i < len(buf):
        buf[i:j] += x[: j - i]


def music(total):
    n = int((total + 1) * SR)
    pads, keys, low = np.zeros(n), np.zeros(n), np.zeros(n)
    bpm = 84
    beat = 60 / bpm
    bar = 4 * beat
    # F major: Fmaj7 · Am7 · Dm9 · Bbmaj7(#11) · then a lift to C/E on the last pass.
    prog = [
        (41, [53, 57, 60, 64], [65, 69, 72, 76, 72, 69, 67, 69]),
        (45, [52, 55, 60, 64], [64, 67, 72, 76, 72, 67, 64, 67]),
        (38, [53, 57, 60, 64], [62, 65, 69, 72, 76, 72, 69, 65]),
        (46, [50, 53, 57, 64], [62, 65, 70, 74, 76, 74, 70, 65]),
    ]
    t, k = 0.0, 0
    while t < total:
        root, chord, arp = prog[k % 4]
        place(pads, pad(chord, 2 * bar + 1.4), t)
        place(low, bass(root, 2 * bar), t)
        # A gentle arpeggio after the intro, thinning out at the end.
        if 2.5 < t < total - 5:
            for i, m in enumerate(arp * 2):
                at = t + i * beat
                swing = 0.012 * (i % 2)
                place(keys, pluck(m, vel=0.55 + 0.25 * (i % 4 == 0)), at + swing)
        # A single bell note marks each new chord.
        place(keys, pluck(arp[0] + 12, 3.0, 0.45), t)
        t += 2 * bar
        k += 1
    lp = butter(2, 2200, "lowpass", fs=SR, output="sos")
    pads = sosfilt(lp, pads)
    keys = sosfilt(butter(2, 5000, "lowpass", fs=SR, output="sos"), keys)
    mix = 0.55 * pads + 0.42 * keys + 0.38 * low
    # Room: a short decaying-noise impulse response.
    ir_n = int(1.8 * SR)
    ir = rng.standard_normal(ir_n) * np.exp(-np.arange(ir_n) / SR * 3.2)
    ir = sosfilt(butter(1, 3000, "lowpass", fs=SR, output="sos"), ir)
    wet = fftconvolve(mix, ir)[:n]
    wet /= np.max(np.abs(wet)) + 1e-9
    mix = mix / (np.max(np.abs(mix)) + 1e-9)
    mix = 0.75 * mix + 0.35 * wet
    n_out = int(total * SR)
    mix = mix[:n_out] * env(n_out, 1.5, 3.5)
    return mix / np.max(np.abs(mix)) * 0.9


def read_wav(p):
    with wave.open(str(p)) as w:
        sr = w.getframerate()
        x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float64) / 32768
    return resample_poly(x, SR, sr) if sr != SR else x


def write_wav(p, x):
    x = np.clip(x, -1, 1)
    st = np.stack([x, x], axis=1)
    with wave.open(str(p), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((st * 32767).astype(np.int16).tobytes())


def main():
    m = music(TOTAL)
    write_wav(PUB / "music.wav", m)
    n = len(m)
    voice = np.zeros(n)
    at = 0.0
    spans = []
    for name, frames in SCENES:
        f = PUB / "vo" / f"{name}.wav"
        if not f.exists():
            # No voice line for this scene: music only.
            at += (frames - OVERLAP) / FPS
            continue
        clip = read_wav(f)
        clip = clip / (np.max(np.abs(clip)) + 1e-9) * 0.92
        start = at + VO_DELAY
        place(voice, clip, start)
        spans.append((start, start + len(clip) / SR))
        at += (frames - OVERLAP) / FPS
    # Duck the music to about a third under speech, easing in and out.
    duck = np.ones(n)
    for a, b in spans:
        i, j = int((a - 0.25) * SR), int((b + 0.35) * SR)
        duck[max(0, i):min(n, j)] = 0.32
    k = int(0.3 * SR)
    duck = np.convolve(duck, np.ones(k) / k, mode="same")
    mix = 0.55 * m * duck + voice
    mix /= max(1.0, np.max(np.abs(mix)) / 0.95)
    write_wav(PUB / "soundtrack.wav", mix)
    print(json.dumps({"seconds": round(TOTAL, 2), "voice": [(round(a, 2), round(b, 2)) for a, b in spans]}))


if __name__ == "__main__":
    main()
