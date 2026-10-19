"""Write a synthetic harness playback (RBRRecompPlayback.bin) for the NGP test install (scripts/ngp-run.sh, notes/ngp.md).

Usage: python3 -I scripts/ngp-synth-playback.py OUT [T0 [HZ [MODE]]]
  T0    first step after the load settle (NGP: 1520; vanilla: 160)
  HZ    steps per second (NGP: 720; vanilla: 100)
  MODE  default: wait with the clutch in, gear up N -> 1st at 6 s, pull away at 12 s
        launch[:TH]: as default with throttle TH (default 0.5) from 5.5 s, so 1st engages under throttle
        blip: as default with throttle 0.5 in neutral at 3-4 s
        inv: as default with the clutch axis inverted
Frames are FRM1 records with only the control payload (car bytes 0). Payload words: 0 throttle, 1 brake,
2 handbrake, 3 clutch, 4 steering, 5 flags (bit 0 gear up), 6 gear, 7 ignition. Times are seconds after T0; the
start hold ignores controls for the first ~1.9 s, so the ignition is held at 2.5-4.5 s.
"""
import struct, sys

IGN0, IGN1, GEAR, GO, SLIP, END = 2.5, 4.5, 6.0, 12.0, 1.0, 32.0


def control(t, gear_step, mode):
    th = br = hb = st = 0.0
    if t < GO:
        cl = 1.0
    elif t < GO + SLIP:
        cl, th = 1.0 - (t - GO) / SLIP, 0.4
    else:
        cl, th = 0.0, 0.6
    if mode == "inv":
        cl = 1.0 - cl
    if mode == "blip" and 3.0 <= t < 4.0:
        th = 0.5
    if mode.startswith("launch") and t >= 5.5:
        th = max(th, float(mode.split(":")[1]) if ":" in mode else 0.5)
    aux = 1 if IGN0 <= t < IGN1 else 0
    return struct.pack("<5fIII", th, br, hb, cl, st, 1 if gear_step else 0, 0, aux)


def main(out, t0="160", hz="100", mode=""):
    t0, hz = int(t0), int(hz)
    with open(out, "wb") as f:
        for s in range(t0, t0 + int(END * hz)):
            f.write(struct.pack("<5I", 0x314D5246, s, s + 1, 1, 0))
            f.write(control((s - t0) / hz, s == t0 + int(GEAR * hz), mode))
            f.write(b"\0" * (4 * 0xA00))


if __name__ == "__main__":
    main(*sys.argv[1:5])
