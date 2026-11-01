"""matrix.py: main-rally stage x car pairs through the Rust autopilot, the NGP original and the Rust check
Resumable: a pair already in the CSV is skipped.

  python3 -I -B harness/matrix.py list                     the 144 default pairs, one spec per line
  python3 -I -B harness/matrix.py run SPECFILE --csv CSV   run the specs in SPECFILE (lines as `list` prints)

NGP 7.4.778's physics (notes/ngp.md "Load path"), from the stage start only, on NGP's four car slots. A spec is
`stage car [weather]`, e.g. `track-12_O 2`; the boot has good weather only. Everything runs here: Rust as
target/release/rbr-standalone (built once at the start), the games as scripts/ngp-run.sh in the NGP install
(RBR_NGP_GAME). RBR_BUSY, a pgrep pattern, adds processes it waits on before a game. Up to two Rust
runs run together; harness games stay serial. Per pair (name ngp-<surface>-<car>-<map>):
  1. df; the batch stops before the runs' disk would go under 100 GB free.
  2. the autopilot at 25 m/s; stuck or off the road: retries at 20 and 16 m/s. It writes the playback in RBR_NGP_RUNS.
     The autopilot presses no ignition (NGP's starter runs on the wall clock, which the port replaces with simulation
     time).
  3. `scripts/ngp-run.sh` twice in the NGP install (drive-<name> and ref-<name>-2 in $RBR_NGP_RUNS, compared as
     V1_REPEAT does: on the static mask, then also on NGP's noise mask), started once no NGP game runs.
  4. `rbr-standalone --boot --ngp-physics` (the install's physics.rbz, $RBR_NGP_GAME/physics.rbz) against
     the drive recording through the last controlled step.
  5. keep $RBR_NGP_RUNS/ngp-matrix/drive-<name>.bin.zst and .auto; delete the rest; one CSV row.
Rerun a check later: zstd -d the .zst next to its .auto and run step 4's command (in the pair's log).
Logs: <csv dir>/ngp-matrix-logs/<name>.log.
NGP steps at 720 Hz; a pair takes up to 130k steps (about 3 minutes of driving), as the harness's recording stops a little
past 2 GiB (133k NGP steps); two pairs per group, the next group's autopilots running with this group's games and checks."""
import concurrent.futures, csv, os, re, shlex, shutil, struct, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dev_env  # noqa: E402  (dev.env: RBR_NGP_GAME, RBR_NGP_RUNS, RBR_BUSY, RBR_DATA)

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(REPO, "target", "release", "rbr-standalone")  # built once at the start
RUNS = os.environ.get("RBR_NGP_RUNS", "")  # absolute (dev.env)
KEEP = RUNS + "/ngp-matrix"
PREFIX = "ngp"
CARS = ["xsara", "accent", "zr", "lancer", "206", "impreza03", "corolla", "impreza00"]
SLOTS = (2, 4, 5, 6)  # the cars NGP's physics.rbz has
STEPS = 130000
GROUP = 2  # pairs whose autopilots and checks run together
GAME = "%s/RichardBurnsRally_SSE" % os.environ.get("RBR_NGP_GAME", "")  # a harness game's command line
NGP_PHYSICS = "%s/physics.rbz" % os.environ.get("RBR_NGP_GAME", "")  # in the NGP install
NGP_SECS = 7200  # a game's time limit
BUSY = os.environ.get("RBR_BUSY", "")  # other processes the games wait for
TYRES = {"gravel": 4, "tarmac": 0, "snow": 6}  # boot.rs default_tyre
FLOOR = 100  # GB free the runs' disk keeps
PAIR = 5  # GB a pair can take at its peak (two 130k-step recordings of 2.1 GB and the playback)
FIELDS = ["stage", "car", "car_name", "surface", "weather", "start", "top_speed", "steps", "finished", "ref1_eq_ref2",
          "rust_equal_compared", "first_difference_or_stop", "wheel_6ec_not_compared", "autopilot_min", "games_min",
          "check_min", "zst_mb", "name", "note", "lane3_masked"]
MIN_STEPS = 2000  # shorter drives are not worth the original's runs


def surface(m):
    """boot.rs `surface` (E14_4a1330) on the main-rally maps."""
    return "snow" if 10 <= m <= 18 else "tarmac" if 41 <= m <= 49 else "gravel"


def variant(letters, weather):
    """The launcher's `fits` (catalog.rs): good weather on a variant other than _O, bad on _O; the first that fits."""
    if len(letters) == 1:
        return letters[0]
    return sorted(l for l in letters if weather == 1 or (l != "O") == (weather == 0))[0]


def stages():
    """The main-rally maps of the launcher catalog (`catalog::title`'s six rallies) and their weather-0 variant."""
    maps = os.path.join(os.environ["RBR_DATA"], "Maps")
    found = {}
    for f in os.listdir(maps):
        g = re.fullmatch(r"track-(\d+)_([A-Z])\.lbs", f)
        if g:
            found.setdefault(int(g[1]), []).append(g[2])
    # Finland 10-15, Britain 20-25, Australia 31-36, France 41-46, Japan 51-56, USA 61-66
    rallies = [range(10, 16), range(20, 26)] + [range(g + 1, g + 7) for g in (30, 40, 50, 60)]
    main = [m for m in sorted(found) if any(m in r for r in rallies)]
    return {m: "track-%d_%s" % (m, variant(sorted(found[m]), 0)) for m in main}


def log(f, text):
    line = "%s %s" % (time.strftime("%H:%M:%S"), text)
    print(line, flush=True)
    f.write(line + "\n")
    f.flush()


def start(f, cmd, env=None):
    """`cmd`, its output going to the log as it runs."""
    log(f, "$ " + shlex.join(cmd))
    at = f.tell()
    return subprocess.Popen(cmd, env=env, stdout=f, stderr=subprocess.STDOUT), at


def finish(f, job):
    """A started command's exit code and output."""
    p, at = job
    rc = p.wait()
    with open(f.name) as g:
        g.seek(at)
        return rc, g.read()


def run(f, cmd, env=None):
    return finish(f, start(f, cmd, env))


def prepare_rust():
    """Build once, before parallel runs share the release binary."""
    subprocess.run(["cargo", "build", "-q", "--release", "--bin", "rbr-standalone",
                    "--manifest-path", os.path.join(REPO, "Cargo.toml")], check=True)


def rust_cmd(args):
    return [BIN] + args + ["--ngp-physics", NGP_PHYSICS]


def free_gb():
    """GB free on the runs' filesystem."""
    return shutil.disk_usage(RUNS if os.path.isdir(RUNS) else "/").free / 1e9


def wait_idle(f):
    """Until no NGP game (any slot, anyone's) has run for 60 s. The processes RBR_BUSY matches count too, and
    once they have run for 5 minutes it asks every 10 minutes."""
    idle, busy_since, said = 0, None, False
    pattern = GAME + ("|" + BUSY if BUSY else "")
    while idle < 3:
        busy = subprocess.run(["pgrep", "-af", pattern], stdout=subprocess.PIPE, text=True).stdout.strip()
        if busy and not said:
            log(f, "a harness game or RBR_BUSY is running; waiting: " + busy.replace("\n", "; "))
            said = True
        idle = 0 if busy else idle + 1
        busy_since = (busy_since or time.time()) if busy else None
        if idle < 3:
            time.sleep(600 if busy_since and time.time() - busy_since > 300 else 30)


def clean(name):
    """This pair's working files by exact name (not the kept ones in KEEP)."""
    files = ("drive-%s-ap.playback.bin drive-%s.playback.bin drive-%s.bin drive-%s.bin.mask drive-%s.auto %s-rec.bin "
             "ref-%s-2.bin drive-%s.harness.log ref-%s-2.harness.log" % ((name,) * 9)).split()
    subprocess.run(["rm", "-f"] + [os.path.join(RUNS, x) for x in files], check=True)


def compared_steps(spec, recording, last_post):
    """The check's step count: the recording's frames from the start through post `last_post`."""
    first = 1000 if spec["base"] else 0
    n, d = 0, open(recording, "rb")
    while True:
        h = d.read(20)
        if len(h) < 20:
            break
        m, pre, post, sub, cs = struct.unpack("<5I", h)
        n += pre >= first and post <= last_post
        d.seek(0x20 + cs + 4 * 0xA00, 1)
    return n


def autopilot(f, spec, out, top):
    args = ["--autopilot", out, "--stage", spec["stage"], "--car", str(spec["car"]), "--steps", str(STEPS),
            "--top-speed", str(top)]
    if spec["base"]:
        args += ["--recording", "%s/drive-%s.bin" % (RUNS, spec["base"]),
                 "--snapshot", "%s/vcu-%s.snap-1000.bin" % (RUNS, spec["base"])]
    else:
        args += ["--boot"]
    rc, text = run(f, rust_cmd(args))
    g = re.search(r": from step (\d+), (\d+) steps driven, finish (step (\d+)|not reached), ended by (.*)", text)
    if not g:
        tail = text.strip().splitlines()[-1:] or ["no output"]
        return {"start": 0, "steps": 0, "finish": None, "end": "autopilot failed: " + tail[0]}
    stop = re.search(r"rbr-standalone: (step \d+: .*?); the car is frozen", text)
    return {"start": int(g[1]), "steps": int(g[2]), "finish": g[4], "end": stop[1] if stop else g[5]}


class Check:
    """A pair's Rust check against its drive recording, from the same start through the last controlled step, running;
    `done` waits for it, keeps the recording and writes the row."""

    def __init__(self, f, spec, row, end, last_post, csv_path):
        self.f, self.row, self.end, self.csv_path = f, row, end, csv_path
        recording = "%s/drive-%s.bin" % (RUNS, row["name"])
        steps = compared_steps(spec, recording, last_post)
        source = ["--snapshot", "%s/vcu-%s.snap-1000.bin" % (RUNS, spec["base"])] if spec["base"] else ["--boot"]
        # /usr/bin/time records this check's wall time while the group's games continue.
        self.job = start(f, ["/usr/bin/time", "-p"] + rust_cmd(["--recording", recording, "--stage", spec["stage"],
                                                                 "--steps", str(steps)] + source))

    def done(self):
        rc, text = finish(self.f, self.job)
        row, name = self.row, self.row["name"]
        g = re.search(r"(\d+) of (\d+) steps run .*?, (\d+) equal to the recording", text)
        if not g:
            eq, first = "-", "check failed: " + (text.strip().splitlines()[-1:] or ["no output"])[0]
        else:
            eq, found = "%s/%s" % (g[3], g[1]), []
            d = re.search(r"differs from the recording after step (\d+): (\d+) words \(([^)]*)\)", text)
            if d:
                found.append("%s @%s (%s words)" % (d[3], d[1], d[2]))
            for pattern in (r"stopped: (.*)", r"(Game's control differs .*)", r"(Game's time card differs .*)"):
                s = re.search(pattern, text)
                if s:
                    found.append(s[1].strip())
            first = "; ".join(found) or "none"
        if self.end:
            first += "; autopilot: " + self.end
        wall = re.findall(r"^real\s+([\d.]+)", text, re.M)
        heap = re.findall(r"(wheel\d\+0x6ec) \((\d+) steps\)", text)
        padding = re.search(r"masked: lane-3 padding \(.*?\): (.*)", text)
        os.makedirs(KEEP, exist_ok=True)
        subprocess.run(["zstd", "-q", "-T4", "-f", "--rm", os.path.join(RUNS, "drive-%s.bin" % name),
                        "-o", os.path.join(KEEP, "drive-%s.bin.zst" % name)], check=True)
        shutil.copy(os.path.join(RUNS, "drive-%s.auto" % name), KEEP)
        zst = os.path.getsize(os.path.join(KEEP, "drive-%s.bin.zst" % name))
        row.update(rust_equal_compared=eq, first_difference_or_stop=first, zst_mb="%.0f" % (zst / 1e6),
                   wheel_6ec_not_compared=", ".join("%s %s steps" % w for w in heap) or "none",
                   lane3_masked=padding[1] if padding else "none",
                   check_min="%.1f" % (float(wall[-1]) / 60) if wall else "-")
        clean(name)
        write_row(self.f, self.csv_path, row)
        self.f.close()


def write_row(f, csv_path, row):
    new = not os.path.exists(csv_path)
    with open(csv_path, "a", newline="") as out:
        w = csv.DictWriter(out, FIELDS)
        if new:
            w.writeheader()
        w.writerow(row)
    log(f, "row: %s" % row)


def prepare_pair(spec, csv_path, reserved_pairs):
    """Set up exact-name files and row metadata before starting this pair's autopilot."""
    car, m = spec["car"], spec["map"]
    surf = surface(m)
    name = "%s-%s-%s-%d%s" % (PREFIX, surf, CARS[car], m, "-w%d" % spec["weather"] if spec["weather"] else "")
    logs = os.path.join(os.path.dirname(csv_path), "%s-matrix-logs" % PREFIX)
    os.makedirs(logs, exist_ok=True)
    f = open(os.path.join(logs, name + ".log"), "a")
    row = dict(stage=spec["stage"], car=car, car_name=CARS[car], surface=surf, weather=spec["weather"], name=name,
               start="snap-1000 of " + spec["base"] if spec["base"] else "boot", note="", zst_mb="-",
               wheel_6ec_not_compared="-", lane3_masked="-", check_min="0", games_min="0")
    free = free_gb()
    log(f, "%s: %.0f GB free" % (name, free))
    if free - PAIR * reserved_pairs < FLOOR:
        log(f, "stop: a floor would be crossed (%.0f GB free, floor %d GB)" % (free, FLOOR))
        f.close()
        return False
    return {"spec": spec, "name": name, "f": f, "row": row}


def run_autopilot(state):
    spec, name, f, row = state["spec"], state["name"], state["f"], state["row"]
    t0 = time.time()
    playback = "%s/drive-%s-ap.playback.bin" % (RUNS, name)
    for top in (25, 20, 16):
        drove = autopilot(f, spec, playback, top)
        if top == 16 or not re.match(r"stuck|off the road", drove["end"]):
            break
        row["note"] += "%d m/s: %s; " % (top, drove["end"])
    row.update(top_speed=top, steps=drove["steps"], finished="step %s" % drove["finish"] if drove["finish"] else "no",
               autopilot_min="%.1f" % ((time.time() - t0) / 60))
    end = None if drove["finish"] and drove["end"].endswith("after the finish") else drove["end"]
    if end:
        row["note"] += "autopilot ended by " + end
    state["drove"], state["end"] = drove, end


def pair(state, csv_path, env):
    """Runs the original for a prepared pair. Returns its Check (running), or None if its row is already written."""
    spec, name, f, row = state["spec"], state["name"], state["f"], state["row"]
    car, m = spec["car"], spec["map"]
    surf = surface(m)
    drove, end = state["drove"], state["end"]
    rc = 0
    if drove["steps"] >= MIN_STEPS:
        t1 = time.time()
        wait_idle(f)
        numbers = "%d %d %d %d 1 0" % (m, car, spec["weather"], TYRES[surf])
        rc, text = ngp_originals(f, name, numbers, env)
        subprocess.run(["rm", "-f", os.path.join(RUNS, "drive-%s-ap.playback.bin" % name),
                        os.path.join(RUNS, "drive-%s.playback.bin" % name)], check=True)
        row["games_min"] = "%.1f" % ((time.time() - t1) / 60)
        repeat = dict(re.findall(r"^repeat (static|noise): (.*)$", text, re.M))
        row["ref1_eq_ref2"] = "static mask: %s; %s noise mask: %s" % (
            repeat.get("static", "no result"), "NGP drive1", repeat.get("noise", "no result"))
    if drove["steps"] >= MIN_STEPS and rc == 0:
        return Check(f, spec, row, end, drove["start"] + drove["steps"], csv_path)
    if drove["steps"] < MIN_STEPS:
        row.update(ref1_eq_ref2="-", rust_equal_compared="-", first_difference_or_stop="autopilot: %s" % end)
    else:
        row.update(rust_equal_compared="-", first_difference_or_stop="ngp-run.sh failed (exit %d)" % rc)
    clean(name)
    write_row(f, csv_path, row)
    f.close()
    return None


def ngp_originals(f, name, numbers, env):
    """The autopilot's playback twice in the NGP install (`scripts/ngp-run.sh`), drive-<name> and ref-<name>-2,
    compared by diverge.py on the static mask, then also on NGP's noise mask (drive1.bin.mask, notes/ngp.md); ref-2
    is deleted. Returns the exit code and the output (with the `repeat` lines pair() reads)."""
    playback = os.path.join(RUNS, "drive-%s-ap.playback.bin" % name)
    renv = dict(env, AUTO=numbers, PLAYBACK=playback)
    for run_name in ("drive-" + name, "ref-%s-2" % name):
        wait_idle(f)
        rc, text = run(f, [os.path.join(REPO, "scripts/ngp-run.sh"), run_name, str(NGP_SECS)], renv)
        if rc:
            return rc, text
    drive, ref = os.path.join(RUNS, "drive-%s.bin" % name), os.path.join(RUNS, "ref-%s-2.bin" % name)
    with open(os.path.join(RUNS, "drive-%s.auto" % name), "w") as g:
        g.write(numbers + "\n")
    div = [sys.executable, "-I", "-B", os.path.join(REPO, "harness/diverge.py")]
    static = subprocess.run(div + [drive, ref], stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True).stdout
    if os.path.lexists(drive + ".mask"):  # a stale one from an interrupted run
        os.remove(drive + ".mask")
    os.symlink(os.path.join(RUNS, "drive1.bin.mask"), drive + ".mask")
    noise = subprocess.run(div + [drive, ref], stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True).stdout
    for p in (drive + ".mask", ref, os.path.join(RUNS, "ref-%s-2.harness.log" % name)):
        if os.path.lexists(p):
            os.remove(p)
    text = "repeat static: %s\nrepeat noise: %s" % ("; ".join(static.splitlines()), "; ".join(noise.splitlines()))
    log(f, text.strip())
    return 0, text


def parse(line):
    words = line.split()
    stage = words[0]
    if len(words) > 3:
        sys.exit("%s: the stage start only (no base)" % line)
    if len(words) == 3 and words[2] != "0":
        sys.exit("%s: the boot has good weather only (no CWeatherDaemon); other weathers need a base" % line)
    m = int(re.fullmatch(r"track-(\d+)_[A-Z]", stage)[1])
    return dict(stage=stage, map=m, car=int(words[1]), weather=int(words[2]) if len(words) > 2 else 0,
                base=words[3] if len(words) > 3 else None)


def main():
    if sys.argv[1:2] == ["list"]:
        for m, stage in stages().items():
            for car in SLOTS:
                print(stage, car)
        return
    if len(sys.argv) < 3 or sys.argv[1] != "run" or "--csv" not in sys.argv:
        sys.exit(__doc__)
    if not os.environ.get("RBR_NGP_GAME") or not RUNS:
        sys.exit("matrix.py: set RBR_NGP_GAME and RBR_NGP_RUNS (dev.env)")
    csv_path = sys.argv[sys.argv.index("--csv") + 1]
    done = set()
    if os.path.exists(csv_path):
        done = {(r["stage"], r["car"], r["weather"]) for r in csv.DictReader(open(csv_path))}
    env = dict(os.environ)
    specs = []
    for line in open(sys.argv[2]):
        line = line.split("#")[0].strip()
        if not line:
            continue
        spec = parse(line)
        if (spec["stage"], str(spec["car"]), str(spec["weather"])) in done:
            print("skip (in the CSV): " + line)
            continue
        specs.append(spec)

    pool = concurrent.futures.ThreadPoolExecutor(max_workers=GROUP)

    def setup(offset, built, held):
        """Prepares the group at `offset` (`held` pairs' recordings still on disk) and starts its autopilots.
        Returns (states, autopilot futures, stop after this group), or None."""
        states, stop_batch = [], False
        for spec in specs[offset:offset + GROUP]:
            state = prepare_pair(spec, csv_path, held + len(states) + 1)
            if state is False:
                stop_batch = True
                break
            states.append(state)
        if not states:
            return None
        if not built:
            # Do not remove files an interrupted harness game could still be writing.
            wait_idle(states[0]["f"])
            for state in states:
                clean(state["name"])
            prepare_rust()
        else:
            wait_idle(states[0]["f"])
            for state in states:
                clean(state["name"])
        free = free_gb()
        if free - PAIR * (held + len(states)) < FLOOR:
            for state in states:
                log(state["f"], "stop: a floor would be crossed (%.0f GB free, floor %d GB)" % (free, FLOOR))
                state["f"].close()
            return None
        # These pairs share a single built release binary, built above before the first group.
        return states, [pool.submit(run_autopilot, state) for state in states], stop_batch

    group = setup(0, False, 0) if specs else None
    offset = 0
    while group:
        states, futures, stop_batch = group
        for future in futures:
            future.result()
        offset += GROUP
        # The next group's autopilots run while this group's games (serial) and checks do.
        group = None if stop_batch else setup(offset, True, len(states))
        checks = []
        for state in states:
            check = pair(state, csv_path, env)
            if check:
                checks.append(check)
        # Their checks finish together, in spec order, before the next group's games.
        for check in checks:
            check.done()
    pool.shutdown()


if __name__ == "__main__":
    main()
