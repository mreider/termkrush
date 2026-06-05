#!/usr/bin/env bash
# Regenerate the synthesized test-audio fixtures under tests/fixtures/.
#
# The fixtures are fully synthetic (sine tones, a log sweep, metronome
# click tracks at known BPMs, and seeded white noise) so they carry no
# licensing encumbrance — they are released CC0 alongside the project —
# and they are deterministic: running this script on any machine produces
# byte-identical WAVs, so the committed fixtures and a regenerated set
# always match.
#
# The committed WAVs are what `cargo test` uses; this script exists only
# so they can be reproduced or extended. WAV is the synthesized source
# format; the manifest records the format per file. One mp3 fixture is also
# produced (by encoding a synthesized WAV with `lame`) so the symphonia
# decode pipeline can assert against a real mp3. mp3 encoders are not
# guaranteed byte-identical across `lame` versions, so the committed
# `.mp3` is the source of truth and this step is best-effort regenerate;
# the synthesized source is CC0, so the mp3 is CC0 too (mp3 patents expired
# in 2017).
#
# Usage: bash scripts/gen-fixtures.sh   (mp3 step needs `lame` on PATH)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT}/tests/fixtures"
mkdir -p "${OUT}"

python3 - "${OUT}" <<'PY'
import math, struct, sys, wave, os

out = sys.argv[1]
SR = 44100  # 44.1 kHz, mono, 16-bit signed PCM throughout.

def write_wav(name, samples):
    path = os.path.join(out, name)
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        frames = bytearray()
        for s in samples:
            v = int(max(-1.0, min(1.0, s)) * 32767.0)
            frames += struct.pack("<h", v)
        w.writeframes(bytes(frames))
    print(f"  wrote {name} ({len(samples)/SR:.1f}s)")

def sine(freq, seconds, amp=0.6):
    n = int(SR * seconds)
    return [amp * math.sin(2 * math.pi * freq * i / SR) for i in range(n)]

def log_sweep(f0, f1, seconds, amp=0.5):
    n = int(SR * seconds)
    out = []
    k = math.log(f1 / f0)
    for i in range(n):
        t = i / SR
        # Instantaneous-phase integral of an exponential chirp.
        phase = 2 * math.pi * f0 * seconds / k * (math.exp(k * t / seconds) - 1)
        out.append(amp * math.sin(phase))
    return out

def click(bpm, seconds, amp=0.9):
    # A short decaying tone burst on every beat — a metronome with a
    # known, exact tempo so BPM-detection tests have ground truth.
    n = int(SR * seconds)
    period = int(SR * 60.0 / bpm)
    burst = int(SR * 0.03)  # 30 ms click
    out = [0.0] * n
    beat = 0
    while beat * period < n:
        start = beat * period
        for j in range(burst):
            idx = start + j
            if idx >= n:
                break
            env = math.exp(-j / (SR * 0.01))  # ~10 ms decay
            out[idx] = amp * env * math.sin(2 * math.pi * 1000.0 * j / SR)
        beat += 1
    return out

def white_noise(seconds, amp=0.4, seed=0x9E3779B1):
    # Deterministic LCG so the committed noise fixture is reproducible.
    n = int(SR * seconds)
    out = []
    state = seed & 0xFFFFFFFF
    for _ in range(n):
        state = (1664525 * state + 1013904223) & 0xFFFFFFFF
        out.append(amp * ((state / 0xFFFFFFFF) * 2.0 - 1.0))
    return out

write_wav("sine_a440_10s.wav", sine(440.0, 10.0))
write_wav("sweep_20_20k_10s.wav", log_sweep(20.0, 20000.0, 10.0))
write_wav("click_120bpm_12s.wav", click(120.0, 12.0))
write_wav("click_128bpm_10s.wav", click(128.0, 10.0))
write_wav("noise_white_5s.wav", white_noise(5.0))
PY

echo "fixtures regenerated in ${OUT}"

# --- mp3 fixture -----------------------------------------------------------
# Encode the synthesized mono 440 Hz sine to a real mp3 so the symphonia
# decode pipeline can assert against actual mp3 frames (duration, RMS,
# mono->stereo upmix). CBR 192 kbps, high-quality mode, mono preserved
# (-m m) so the decoder's mono->stereo path is exercised. The committed
# .mp3 is the source of truth; lame versions may differ byte-for-byte.
if command -v lame >/dev/null 2>&1; then
  lame --quiet -m m -b 192 -q 2 \
    "${OUT}/sine_a440_10s.wav" "${OUT}/sine_a440_10s.mp3"
  echo "  encoded sine_a440_10s.mp3 (lame: $(lame --version 2>&1 | head -1))"
else
  echo "  SKIP mp3: 'lame' not on PATH. Install it (brew install lame) to" >&2
  echo "       regenerate tests/fixtures/sine_a440_10s.mp3; the committed" >&2
  echo "       file remains the source of truth." >&2
fi
