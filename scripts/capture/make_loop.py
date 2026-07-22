#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy"]
# ///
"""Synthesize the rights-clear demo loop used to capture Fosfora's README media.

Everything here is generated from oscillators and noise, so the resulting WAV can be
committed, published and re-derived freely. It is not trying to be a good track — it is
trying to be a track that lights up every detector in the analysis pipeline, deliberately
and in a known order, so a capture run exercises real audio reactivity rather than a
flat drone.

The loop is seamless and one effect is filmed per loop pass, so the capture run phase-locks to
it and every effect is filmed over the identical musical passage. That is what makes the gallery
a fair comparison instead of a lottery over whichever bar happened to be playing.

What each section is for, in analyzer terms — see `audio/structure.rs` for the detector this
is written against (20 bars ≈ 38.7 s at 124 BPM):

  bars  1-8   groove    kick, sub, hats, pad   kick / onset / beat / bpm lock, percussive_energy
  bars  9-15  buildup   gain ramp, opening     `buildup` climbs: loudness_trend rises, centroid
                        filter, accelerating   rises above its 8 s mean, onset density rises,
                        roll, riser, kick out  and the sub withdraws (w_subbass = 1.6)
  bar  15     gap       ~a beat of near        drops the 1.5 s running-minimum loudness baseline
                        silence                so the drop's `jump` clears 0.08 with room to spare
  bars 16-20  drop      everything, sub back   `drop` fires: armed + loud jump + sub-bass return

Measured against the real analyzer over two loop passes: BPM locks 122-124, buildup peaks 0.85
and holds above the 0.6 arm threshold for 6.3 s (it needs 4.0 s), drop fires exactly once per
pass, and HPSS separates cleanly (percussive 0.93 / harmonic 0.96).

Two things that look like production choices but are really detector requirements: the buildup
is LONG, because `buildup` is driven by an 8 s loudness slope and has to hold high for 4 s
before the drop arms; and the kick leaves entirely before the drop, because a kick pitched to
44 Hz sits in the sub-bass band and keeps the sub-withdrawal term pinned at zero if it stays.

Chords change every two bars (Am - F - C - G) so chroma and key move rather than sitting still.
A filter sweep rides the pad so centroid/flatness/rolloff are never static, and the highs get a
Haas spread so pan/stereo_width/stereo_corr aren't dead either.

Usage:  ./make_loop.py [-o loop.wav] [--bpm 124] [--bars 20]
"""

from __future__ import annotations

import argparse
import struct
import wave
from pathlib import Path

import numpy as np

SR = 48_000
RNG = np.random.default_rng(0x50F0)  # fixed seed: capture runs must be reproducible


# --------------------------------------------------------------------------- helpers


def env(n: int, attack: float, decay: float, curve: float = 2.0) -> np.ndarray:
    """Percussive attack/decay envelope over `n` samples (seconds for attack/decay)."""
    a = max(1, int(attack * SR))
    d = max(1, int(decay * SR))
    out = np.zeros(n, dtype=np.float32)
    head = min(a, n)
    out[:head] = np.linspace(0.0, 1.0, head, dtype=np.float32)
    tail = min(d, max(0, n - head))
    if tail:
        out[head : head + tail] = np.linspace(1.0, 0.0, tail, dtype=np.float32) ** curve
    return out


def onepole_lp(x: np.ndarray, cutoff: np.ndarray | float) -> np.ndarray:
    """One-pole lowpass with a per-sample cutoff in Hz. Cheap, but it sweeps, which is the point."""
    c = np.broadcast_to(np.asarray(cutoff, dtype=np.float32), x.shape)
    alpha = 1.0 - np.exp(-2.0 * np.pi * np.clip(c, 20.0, SR * 0.45) / SR)
    out = np.empty_like(x)
    acc = 0.0
    # Sample loop: the cutoff is modulated, so this cannot be a single lfilter call.
    for i in range(x.size):
        acc += alpha[i] * (x[i] - acc)
        out[i] = acc
    return out.astype(np.float32)


def onepole_hp(x: np.ndarray, cutoff: float) -> np.ndarray:
    return (x - onepole_lp(x, cutoff)).astype(np.float32)


def add(buf: np.ndarray, sig: np.ndarray, at: int, gain: float = 1.0) -> None:
    """Mix `sig` into `buf` at sample offset `at`, clipping the tail at the buffer end."""
    if at >= buf.size:
        return
    n = min(sig.size, buf.size - at)
    buf[at : at + n] += sig[:n] * gain


# --------------------------------------------------------------------------- voices


def kick(dur: float = 0.42) -> np.ndarray:
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    # Pitch drop 118 Hz -> 44 Hz: the sub-bass thump the kick-band detector keys on.
    f = 44.0 + 74.0 * np.exp(-t * 38.0)
    body = np.sin(2 * np.pi * np.cumsum(f) / SR) * env(n, 0.001, dur, 2.4)
    click = onepole_hp(RNG.normal(0, 1, n).astype(np.float32), 1800.0) * env(n, 0.0002, 0.012, 3.0)
    return np.tanh(body * 1.6 + click * 0.35).astype(np.float32)


def snare(dur: float = 0.20, bright: float = 1.0) -> np.ndarray:
    n = int(dur * SR)
    noise = RNG.normal(0, 1, n).astype(np.float32)
    body = onepole_hp(noise, 1200.0 * bright) * env(n, 0.001, dur, 2.0)
    tone = np.sin(2 * np.pi * 190.0 * np.arange(n, dtype=np.float32) / SR) * env(n, 0.001, 0.09, 2.0)
    return (body * 0.8 + tone * 0.4).astype(np.float32)


def hat(dur: float = 0.055, open_: bool = False) -> np.ndarray:
    d = dur * (4.5 if open_ else 1.0)
    n = int(d * SR)
    noise = RNG.normal(0, 1, n).astype(np.float32)
    return (onepole_hp(noise, 7000.0) * env(n, 0.0005, d, 2.6)).astype(np.float32)


def sub(freq: float, dur: float) -> np.ndarray:
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    # Sine plus a soft octave-up for definition on small speakers.
    s = np.sin(2 * np.pi * freq * t) + 0.22 * np.sin(4 * np.pi * freq * t)
    e = env(n, 0.006, dur, 1.2)
    return np.tanh(s * e * 1.3).astype(np.float32)


def pad(freqs: list[float], dur: float, cutoff: np.ndarray | float) -> np.ndarray:
    """Detuned saw stack through a sweeping lowpass — the harmonic bed HPSS should isolate."""
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    out = np.zeros(n, dtype=np.float32)
    for f in freqs:
        for detune in (-0.16, 0.0, 0.19):
            ph = ((f + detune) * t) % 1.0
            out += (2.0 * ph - 1.0).astype(np.float32)  # naive saw; the filter tames the aliasing
    out /= len(freqs) * 3
    e = np.minimum(env(n, 0.25, dur, 0.7) * 3.0, 1.0)
    return (onepole_lp(out, cutoff) * e).astype(np.float32)


def stab(freqs: list[float], dur: float = 0.34) -> np.ndarray:
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    out = np.zeros(n, dtype=np.float32)
    for f in freqs:
        out += np.sin(2 * np.pi * f * t) + 0.4 * np.sin(4 * np.pi * f * t)
    out /= len(freqs)
    return (onepole_lp(out, 3800.0) * env(n, 0.004, dur, 2.2)).astype(np.float32)


def riser(dur: float) -> np.ndarray:
    """Noise through a rising highpass plus a rising tone — pure buildup food."""
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    ramp = (t / dur).astype(np.float32)
    noise = RNG.normal(0, 1, n).astype(np.float32)
    swept = noise - onepole_lp(noise, 300.0 + 6500.0 * ramp**2)
    tone = np.sin(2 * np.pi * np.cumsum(220.0 + 1400.0 * ramp**3) / SR).astype(np.float32)
    return ((swept * 0.7 + tone * 0.3) * (0.08 + 0.92 * ramp**2)).astype(np.float32)


# --------------------------------------------------------------------------- arrangement

# A minor -> F -> C -> G. Two bars each, so chroma/key have time to settle and then move.
NOTE = {"A": 55.00, "C": 65.41, "E": 82.41, "F": 87.31, "G": 98.00, "B": 61.74, "D": 73.42}
PROGRESSION = [
    ("A", ["A", "C", "E"]),
    ("F", ["F", "A", "C"]),
    ("C", ["C", "E", "G"]),
    ("G", ["G", "B", "D"]),
]


def build(bpm: float, bars: int) -> np.ndarray:
    beat = 60.0 / bpm
    bar = beat * 4
    # Render with a bar of overhang, then fold it back onto the head (see below) so voices that
    # ring past the final bar land on the loop point instead of being guillotined into a click.
    total = int(bars * bar * SR)
    buf = np.zeros(total + int(bar * SR), dtype=np.float32)

    def at(bar_i: float) -> int:
        return int(bar_i * bar * SR)

    # Section boundaries. The buildup is deliberately LONG: `drop` only arms once `buildup`
    # has held above 0.6 for 4 s (DROP_ARM_SUSTAIN), and buildup is driven largely by
    # loudness_trend, itself a slope over an 8 s window. A short buildup peaks and passes
    # before the arming timer fills — measured at 0.628 peak with a 6-bar buildup, which
    # grazed the threshold and never fired a drop.
    build_start = round(bars * 0.40)
    drop_start = round(bars * 0.75)

    for b in range(bars):
        root, chord = PROGRESSION[(b // 2) % len(PROGRESSION)]
        in_build = build_start <= b < drop_start
        in_drop = b >= drop_start
        prog = (b - build_start) / max(1, drop_start - build_start)  # 0..1 through the buildup

        # Buildup gain ramp. loudness_trend is a slope over 8 s, so the section has to get
        # genuinely louder as it goes — starting the buildup well *below* the groove is what
        # makes the rise readable rather than a plateau. The exponent is < 1 so the level is
        # already high two-thirds of the way in, giving `buildup` time to sit above 0.6 for
        # the 4 s the drop needs rather than only touching it at the very end.
        ramp = 0.42 + 0.76 * np.clip(prog, 0, 1) ** 0.7 if in_build else 0.92

        # --- pad: always present; filter opens through the buildup, wide open on the drop
        if in_build:
            cut = 800.0 + 6900.0 * np.clip(prog, 0, 1) ** 0.9
        elif in_drop:
            cut = 7600.0
        else:
            cut = 1500.0
        pad_freqs = [NOTE[n] * 4 for n in chord]  # two octaves up, out of the sub's way
        add(buf, pad(pad_freqs, bar * 1.02, cut), at(b), (0.30 if in_drop else 0.24) * ramp)

        # --- hats: eighths, sixteenths through the buildup
        div = 8 if in_build else 4
        for i in range(div):
            openish = (i % 4) == 2 and not in_build
            g = (0.10 if i % 2 else 0.16) * ramp
            add(buf, hat(open_=openish), at(b) + int(i * bar / div * SR), g)

        # --- kick and sub: both leave through the buildup, the kick LAST and completely.
        #
        # The `buildup` logistic's joint-largest term (w_subbass = 1.6) measures how far the
        # sub-bass band sits below its ~8 s average. A kick pitched down to 44 Hz lives squarely
        # in that band (20-60 Hz), so thinning the kick to two hits a bar still held sub_bass at
        # a 0.73 mean and the withdrawal term never registered — buildup stalled at 0.647 and
        # the drop never armed. Pulling the kick entirely for the back of the buildup is both
        # what the detector needs and what the genre does anyway, and it makes the drop's
        # sub-bass *return* unmistakable.
        kick_hits = 4
        if in_build:
            kick_hits = 4 if prog < 0.3 else (2 if prog < 0.55 else 0)
        for i in range(kick_hits):
            stride = 4 // kick_hits
            add(buf, kick(), at(b) + int(i * stride * beat * SR), (0.95 if in_drop else 0.85) * ramp)

        if not (in_build and prog >= 0.3):
            for i in (0, 2):
                add(buf, sub(NOTE[root], beat * 1.6), at(b) + int(i * beat * SR), 0.55 * ramp)

        # --- backbeat snare
        for i in (1, 3):
            add(buf, snare(), at(b) + int(i * beat * SR), 0.34 * ramp)

        # --- buildup: accelerating snare roll
        if in_build:
            rate = 2 ** int(1 + 3 * np.clip(prog, 0, 0.999))  # 2 -> 4 -> 8 -> 16 per bar
            for i in range(rate):
                g = (0.14 + 0.24 * (i / rate)) * ramp
                add(buf, snare(0.13, 1.0 + prog), at(b) + int(i * bar / rate * SR), g)

        # --- drop: chord stabs on the offbeats
        if in_drop:
            for i in (1, 2, 3):
                add(buf, stab(pad_freqs), at(b) + int((i + 0.5) * beat * SR), 0.20)

    # One long riser across the buildup.
    add(buf, riser((drop_start - build_start) * bar), at(build_start), 0.42)

    # Pre-drop gap: duck the last beat before the drop. The drop detector measures its loudness
    # jump against a running MINIMUM over the last 1.5 s (DROP_BASELINE_SECONDS), and that
    # minimum comes from `loudness_m` — an EBU *momentary* meter with a 400 ms window. So the
    # gap has to be comfortably longer than 400 ms or the meter never sees the bottom of it.
    gap_end = at(drop_start)
    gap_start = gap_end - int(0.95 * beat * SR)
    fade = np.linspace(1.0, 0.015, gap_end - gap_start, dtype=np.float32) ** 3
    buf[gap_start:gap_end] *= fade

    # Fold the overhang back onto the head: a voice still ringing at the end of bar 16 continues
    # into bar 1 of the next pass, exactly as it would if the loop were played twice.
    tail = buf[total:]
    buf[: tail.size] += tail
    return buf[:total]


def stereoize(mono: np.ndarray) -> np.ndarray:
    """Widen with a short Haas delay on the highs so pan/stereo_width/stereo_corr aren't dead."""
    hi = onepole_hp(mono, 900.0)
    d = int(0.010 * SR)
    left = mono.copy()
    right = mono.copy()
    left[d:] += hi[:-d] * 0.22
    right[:-d] += hi[d:] * 0.22
    return np.stack([left, right], axis=1)


def normalize(x: np.ndarray, peak_db: float = -1.0) -> np.ndarray:
    peak = float(np.max(np.abs(x))) or 1.0
    return (x / peak * (10 ** (peak_db / 20.0))).astype(np.float32)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("-o", "--out", type=Path, default=Path("loop.wav"))
    ap.add_argument("--bpm", type=float, default=124.0)
    ap.add_argument("--bars", type=int, default=20)
    args = ap.parse_args()

    mono = build(args.bpm, args.bars)
    stereo = normalize(stereoize(mono))
    pcm = (np.clip(stereo, -1.0, 1.0) * 32767.0).astype(np.int16)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(args.out), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(struct.pack(f"<{pcm.size}h", *pcm.flatten().tolist()))

    dur = stereo.shape[0] / SR
    print(f"wrote {args.out}  {dur:.1f}s  {args.bpm:g} BPM  {args.bars} bars")


if __name__ == "__main__":
    main()
