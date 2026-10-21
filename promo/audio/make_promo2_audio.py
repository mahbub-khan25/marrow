"""Original music and sound effects for the second promo (MIT licensed).

    python audio/make_promo2_audio.py

Reads src/v2/timeline.json so every click, pop and whoosh lands on its frame,
and writes public/promo2-audio.wav (48 kHz stereo). 120 BPM, D major.
"""
import json
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, fftconvolve, sosfilt

ROOT = Path(__file__).resolve().parent.parent
T = json.loads((ROOT / "src/v2/timeline.json").read_text())
SR = 48000
FPS = T["fps"]
LEN = T["total"] / FPS
N = int(LEN * SR)
BEAT = 60 / T["bpm"]
BAR = BEAT * 4
rand = np.random.default_rng(11)


def sec(frame):
    return frame / FPS


def lp(x, hz, order=2):
    return sosfilt(butter(order, hz, "low", fs=SR, output="sos"), x)


def hp(x, hz, order=2):
    return sosfilt(butter(order, hz, "high", fs=SR, output="sos"), x)


def bp(x, lo, hi, order=2):
    return sosfilt(butter(order, [lo, hi], "band", fs=SR, output="sos"), x)


def midi(m):
    return 440 * 2 ** ((m - 69) / 12)


def place(buf, clip, t, gain=1.0):
    i = int(t * SR)
    if i >= len(buf) or i + len(clip) <= 0:
        return
    j = min(len(buf), i + len(clip))
    s = max(0, -i)
    buf[max(0, i):j] += clip[s : j - i] * gain


def tt(d):
    return np.arange(int(d * SR)) / SR


# ------------------------------------------------------------ instruments

def kick():
    t = tt(0.35)
    f = 45 + 80 * np.exp(-t * 30)
    ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * np.exp(-t * 9) + 0.15 * np.sin(ph * 2) * np.exp(-t * 30)


def clap():
    t = tt(0.25)
    n = bp(rand.standard_normal(len(t)), 900, 4000) * np.exp(-t * 22)
    return n * 0.9


def hat(open_=False):
    t = tt(0.18 if open_ else 0.05)
    return hp(rand.standard_normal(len(t)), 7000) * np.exp(-t * (18 if open_ else 70)) * 0.35


def pluck(f, d=0.35):
    t = tt(d)
    env = np.exp(-t * 9) * (1 - np.exp(-t * 400))
    x = np.sin(2 * np.pi * f * t) + 0.35 * np.sin(4 * np.pi * f * t) + 0.12 * np.sin(6 * np.pi * f * t)
    return x * env


def bass(f, d):
    t = tt(d)
    x = np.tanh(2.2 * (np.sin(2 * np.pi * f * t) + 0.3 * np.sin(4 * np.pi * f * t)))
    return lp(x, 600) * np.exp(-t * 3.5) * (1 - np.exp(-t * 300))


def pad(freqs, d, detune):
    t = tt(d)
    x = np.zeros(len(t))
    for f in freqs:
        for k in range(1, 7):
            x += np.sin(2 * np.pi * f * (1 + detune) * k * t + k) / k
    env = np.minimum(1, t / 0.25) * np.minimum(1, (d - t) / 0.3)
    return lp(x * env, 2200) / len(freqs)


# D, A, Bm, G
CHORDS = [[62, 66, 69], [61, 64, 69], [62, 66, 71], [62, 67, 71]]
ROOTS = [38, 33, 35, 31]


def energy(bar):
    """0 intro, 1 light, 2 groove, 3 full, 4 half-time, 5 outro."""
    s = {k: (v[0] / FPS / BAR, v[1] / FPS / BAR) for k, v in T["scenes"].items()}
    if bar < s["files"][0]:
        return 0
    if bar < s["write"][0]:
        return 1
    if bar < s["speed"][0]:
        return 2
    if bar < s["safe"][0]:
        return 3
    if bar < s["features"][0]:
        return 4
    if bar < s["cta"][0]:
        return 3
    return 5


L = np.zeros(N)
R = np.zeros(N)
drums = np.zeros(N)
bassb = np.zeros(N)
padL = np.zeros(N)
padR = np.zeros(N)
arp = np.zeros(N)

bars = int(np.ceil(LEN / BAR))
K, CL = kick(), clap()
for b in range(bars):
    t0 = b * BAR
    e = energy(b)
    ci = b % 4 if e else (2 if b % 2 == 0 else 3)
    chord = CHORDS[ci]
    # Pad on every bar, stereo detuned.
    pg = 0.55 if e in (0, 5) else 0.4
    place(padL, pad([midi(m) for m in chord], BAR + 0.3, -0.0012), t0, pg)
    place(padR, pad([midi(m) for m in chord], BAR + 0.3, 0.0012), t0, pg)
    for s16 in range(16):
        t = t0 + s16 * BEAT / 4
        beat, sub = divmod(s16, 4)
        # Arpeggio
        if e in (1, 2, 3, 5) or (e == 0 and sub == 0) or (e == 4 and sub % 2 == 0):
            notes = chord + [m + 12 for m in chord]
            m = notes[[0, 1, 2, 3, 4, 5, 4, 2][s16 % 8]]
            g = {0: 0.18, 1: 0.22, 2: 0.26, 3: 0.3, 4: 0.22, 5: 0.24}[e]
            place(arp, pluck(midi(m + 12)), t, g * (1 if sub == 0 else 0.7))
        # Drums
        if e in (2, 3) and sub == 0:
            place(drums, K, t, 0.95)
        if e == 1 and sub == 0 and beat in (0, 2):
            place(drums, K, t, 0.8)
        if e == 4 and sub == 0 and beat == 0:
            place(drums, K, t, 0.8)
        if e in (2, 3) and sub == 0 and beat in (1, 3):
            place(drums, CL, t, 0.55)
        if e == 4 and sub == 0 and beat == 2:
            place(drums, CL, t, 0.5)
        if e in (1, 2, 3) and sub == 2:
            place(drums, hat(e == 3 and beat == 3), t, 0.9)
        if e == 3 and sub in (1, 3):
            place(drums, hat(), t, 0.45)
        if e == 0 and sub == 0:
            place(drums, hat(), t, 0.5)  # clock tick in the intro
        # Bass
        if e in (1, 2, 3) and sub in (0, 2):
            place(bassb, bass(midi(ROOTS[ci] + (12 if e == 3 and sub == 2 and beat % 2 else 0)), BEAT / 2), t, 0.45)
        if e == 4 and sub == 0 and beat in (0, 2):
            place(bassb, bass(midi(ROOTS[ci]), BEAT * 2), t, 0.6)

# Side-chain: duck pads and bass under each kick.
duck = np.ones(N)
env = 1 - 0.55 * np.exp(-tt(0.3) * 12)
kick_times = np.where(np.abs(drums) > 0.6)[0]
last = -SR
for i in kick_times:
    if i - last > SR * 0.2:
        j = min(N, i + len(env))
        duck[i:j] = np.minimum(duck[i:j], env[: j - i])
        last = i

# Simple reverb for pads and arp.
ir = rand.standard_normal(int(SR * 1.4)) * np.exp(-tt(1.4) * 4.5)
ir = lp(ir, 5000)
ir *= 0.2 / np.sqrt(np.sum(ir**2))
rev = fftconvolve(arp + 0.5 * (padL + padR), ir)[:N]

music_L = 0.7 * padL * duck + bassb * duck + drums + arp * 1.5 + rev
music_R = 0.7 * padR * duck + bassb * duck + drums + arp * 1.5 + rev
# Fade the music out over the last two seconds.
fade = np.ones(N)
fade[-SR * 2 :] = np.linspace(1, 0, SR * 2) ** 1.5
music_L *= fade
music_R *= fade

# ------------------------------------------------------------ effects

fx = np.zeros(N)


def keyclick():
    t = tt(0.03)
    return (bp(rand.standard_normal(len(t)), 1800, 6000) * np.exp(-t * 260) + 0.4 * np.sin(2 * np.pi * 1200 * t) * np.exp(-t * 400)) * rand.uniform(0.55, 1)


def pop():
    t = tt(0.12)
    f = 650 + 900 * (1 - np.exp(-t * 60))
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 28)


def ding():
    t = tt(0.9)
    return (np.sin(2 * np.pi * 1318.5 * t) + 0.6 * np.sin(2 * np.pi * 1975.5 * t)) * np.exp(-t * 6) * (1 - np.exp(-t * 900))


def whoosh(d=0.6):
    t = tt(d)
    shape = np.sin(np.pi * t / d) ** 2
    return bp(rand.standard_normal(len(t)), 300, 5000) * shape


def impact():
    t = tt(1.6)
    boom = np.sin(2 * np.pi * (40 + 40 * np.exp(-t * 8)) * t) * np.exp(-t * 3)
    hit = lp(rand.standard_normal(len(t)), 3000) * np.exp(-t * 14) * 0.5
    return boom + hit


def zip_(d):
    t = tt(d)
    f = 250 * (8 ** (t / d))
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.minimum(1, t / 0.02) * np.exp(-((t - d) ** 2) / 0.02) * 0.8


for seg in T["typing"]:
    for i in range(len(seg["text"])):
        place(fx, keyclick(), sec(seg["start"] + i * seg["rate"]), 0.5)
for f in T["dings"]:
    place(fx, ding(), sec(f), 0.32)
for f in T["pops"]:
    place(fx, pop(), sec(f), 0.4)
for f in T["impacts"]:
    place(fx, impact(), sec(f), 0.7)
for f in T["zip"]:
    place(fx, zip_(0.45), sec(f), 0.35)
for k, (a, _) in T["scenes"].items():
    if a > 0:
        place(fx, whoosh(0.6), sec(a) - 0.4, 0.22)
for f in T["swipes"]:
    place(fx, whoosh(0.35), sec(f) - 0.2, 0.16)
# Riser into "Meet Marrow".
r = tt(1.2)
place(fx, hp(rand.standard_normal(len(r)), 2000) * (r / 1.2) ** 3 * 0.35, sec(T["impacts"][0]) - 1.2)

outL = music_L * 0.7 + fx
outR = music_R * 0.7 + fx
# Gentle bus compression and limiting.
st = np.stack([outL, outR], axis=1)
for name, x in [("drums", drums), ("bass", bassb * duck), ("pad", padL * duck), ("arp", arp), ("reverb", rev), ("fx", fx)]:
    print(f"  {name:7s} RMS {20 * np.log10(np.sqrt(np.mean(x**2)) + 1e-9):6.1f} dB, peak {np.max(np.abs(x)):.2f}")
# Aim for about -16 dBFS RMS, then soft-limit the peaks below 0.9.
st *= 10 ** ((-16 - 20 * np.log10(np.sqrt(np.mean(st**2)))) / 20)
a = np.abs(st)
knee = 0.75
st = np.where(a > knee, np.sign(st) * (knee + (0.9 - knee) * np.tanh((a - knee) / (0.9 - knee))), st)
with wave.open(str(ROOT / "public/promo2-audio.wav"), "wb") as w:
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(SR)
    w.writeframes((st * 32767).astype(np.int16).tobytes())
rms = 20 * np.log10(np.sqrt(np.mean(st**2)))
print(f"{LEN:.2f} s, RMS {rms:.1f} dBFS")
