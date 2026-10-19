"""matrix.py: main-rally stage x car pairs through the Rust autopilot, the original and the Rust check
(lead/reports/v7-matrix.md). Resumable: a pair already in the CSV is skipped.

  python3 -I -B harness/matrix.py list [--ngp]                     the 288 default pairs, one spec per line
  python3 -I -B harness/matrix.py run SPECFILE [--ngp] [--csv CSV]   run the specs in SPECFILE (lines as `list` prints)

A spec is `stage car [weather [base]]`, e.g. `track-12_O 2` or `track-23_E 3 1 v3-gravel-lancer-23`. Without a base the
pair starts from the stage start (`rbr-standalone --autopilot --boot`; the boot has good weather only); with one it
starts from the base drive's @1000 snapshot (cachy:rbr-harness-runs/vcu-<base>.snap-1000.bin and drive-<base>.bin).
Everything runs on cachy. Up to four Rust runs run together; harness games stay serial. Per pair
(name v7-<surface>-<car>-<map>[-w<weather>]):
  1. df on both machines; the batch stops before cachy would go under 100 GB free (or when the Mac is under 40 GB).
  2. the autopilot at 25 m/s; stuck or off the road: retries at 20 and 16 m/s. It writes the playback on cachy.
  3. `V1_REPEAT=1 harness/v1-drive.sh` (the original twice: drive-<name>.bin and ref-2, compared on the static mask,
     then also on tanner-corolla's noise mask), started once no harness game has run on cachy for 60 s.
  4. `rbr-standalone --boot` (or `--snapshot`) against the drive recording through the last controlled step.
  5. keep cachy:rbr-harness-runs/v7-matrix/drive-<name>.bin.zst and .auto; delete the rest; one CSV row.
Rerun a check later: zstd -d the .zst next to its .auto on cachy and run step 4's command (in the pair's log).
Logs: <csv dir>/v7-matrix-logs/<name>.log.

--ngp: NGP 7.4.778's physics (notes/ngp-load-path.md), from the stage start only, on NGP's four car slots (144 default
pairs, name ngp-<surface>-<car>-<map>, CSV ngp-matrix.csv). The Rust runs take `--ngp-physics` (the test install's
physics.rbz); step 3 is `scripts/ngp-run.sh` twice in the NGP test install (drive-<name> and ref-<name>-2 in
cachy:rbr-ngp-runs, compared as V1_REPEAT does), started once no NGP game runs; kept in rbr-ngp-runs/ngp-matrix. NGP
steps at 720 Hz; a pair takes up to 130k steps (about 3 minutes of driving), as the harness's recording stops a little
past 2 GiB (133k NGP steps); two pairs per group, the next group's autopilots running with this group's games and checks. The autopilot presses no ignition (NGP's
starter runs on the wall clock, which the port replaces with simulation time)."""
import concurrent.futures, csv, os, re, shlex, shutil, subprocess, sys, time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REMOTE_RUST = "rbr-rust/src/%s" % os.path.basename(REPO)
REMOTE_BIN = "~/%s/target/release/rbr-standalone" % REMOTE_RUST
DEFAULT_CSV = os.path.realpath(os.path.join(REPO, "..", "lead/reports/v7-matrix.csv"))
RUNS = "rbr-harness-runs"  # on cachy, relative to its home
KEEP = RUNS + "/v7-matrix"
PREFIX = "v7"
CARS = ["xsara", "accent", "zr", "lancer", "206", "impreza03", "corolla", "impreza00"]
SLOTS = range(8)
STEPS = 150000
GROUP = 4  # pairs whose autopilots and checks run together
GAME = "rbr-vanilla-1[.]02.*RichardBurnsRally_SSE"  # a harness game's command line on cachy
NGP = False
NGP_PHYSICS = "Games/rbr-ngp-7.4.778/physics.rbz"  # on cachy
NGP_SECS = 7200  # a game's time limit
USER_VIEWER = "[r]br-rust/src/play/target"  # the user's viewer and launcher on cachy (--ngp waits for them)
TYRES = {"gravel": 4, "tarmac": 0, "snow": 6}  # boot.rs default_tyre
MAC_FLOOR, CACHY_FLOOR = 40, 100  # GB free
CACHY_PAIR = 10  # GB a pair can take on cachy at its peak (150k steps: 2.3 GB per copy, four copies)
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


def ssh(cmd):
    """`cmd` in bash on cachy (its login shell is fish)."""
    return subprocess.run(["ssh", "-o", "BatchMode=yes", "cachy", "bash -s"], input=cmd, stdout=subprocess.PIPE,
                          text=True, check=True).stdout


def prepare_rust():
    """Sync and build once, before parallel runs share the release binary."""
    subprocess.run(["rsync", "-a", "--delete", "--exclude", "target", os.path.join(REPO, "rust/"),
                    "cachy:%s/" % REMOTE_RUST], check=True)
    ssh("nice -n 10 ~/.cargo/bin/cargo build -q --release -j 4 --manifest-path ~/%s/Cargo.toml --bin rbr-standalone" %
        REMOTE_RUST)


def rust_cmd(args):
    if NGP:
        args = args + ["--ngp-physics", NGP_PHYSICS]
    remote = "cd ~ && RBR_DATA=~/rbr-rust/data/retail-install RBR_PATCH=~/rbr-rust/data/Main_group " \
             "nice -n 10 %s %s" % (REMOTE_BIN, shlex.join(args))
    return ["ssh", "-o", "BatchMode=yes", "cachy", "bash -c " + shlex.quote(remote)]


def free_gb():
    mac = shutil.disk_usage("/").free / 1e9
    cachy = int(ssh("df -BG --output=avail ~ | tail -1").strip().rstrip("G"))
    return mac, cachy


def wait_cachy_rust_idle():
    """Let Rust jobs or builds from an interrupted invocation finish before cleanup or another build."""
    pattern = "[r]br-rust/src/%s" % os.path.basename(REPO)
    said = False
    while ssh("pgrep -f '%s' || true" % pattern).strip():
        if not said:
            print("cachy: Rust work from this matrix worktree is still active; waiting", flush=True)
            said = True
        time.sleep(30)


def wait_cachy_idle(f):
    """Until no harness game (any slot, anyone's; NGP's under --ngp) has run for 60 s; the pattern never matches the
    user's own install. Under --ngp the user's NGP games (the same install) and viewer (USER_VIEWER) count too, and
    once they have run for 5 minutes it asks every 10 minutes."""
    idle, busy_since, said = 0, None, False
    pattern = GAME + ("|" + USER_VIEWER if NGP else "")
    while idle < 3:
        busy = ssh("pgrep -af '%s' | cut -c1-160 || true" % pattern).strip()
        if busy and not said:
            log(f, "cachy: a harness game or the user's viewer is running; waiting: " + busy.replace("\n", "; "))
            said = True
        idle = 0 if busy else idle + 1
        busy_since = (busy_since or time.time()) if busy else None
        if idle < 3:
            time.sleep(600 if NGP and busy_since and time.time() - busy_since > 300 else 30)


def clean(name):
    """This pair's working files on cachy by exact name (not the kept ones in KEEP)."""
    ssh("cd ~/%s && rm -f drive-%s-ap.playback.bin drive-%s.playback.bin drive-%s.bin drive-%s.bin.mask drive-%s.auto "
        "%s-rec.bin ref-%s-2.bin drive-%s.harness.log ref-%s-2.harness.log" % ((RUNS,) + (name,) * 9))


def compared_steps(spec, recording, last_post):
    """The check's step count: the recording's frames from the start through post `last_post` (read on cachy)."""
    first = 1000 if spec["base"] else 0
    return int(ssh("""python3 -I -B - <<'EOF'
import struct
d = open("%s", "rb"); n = 0
while True:
    h = d.read(20)
    if len(h) < 20: break
    m, pre, post, sub, cs = struct.unpack("<5I", h)
    n += pre >= %d and post <= %d
    d.seek(0x20 + cs + 4 * 0xa00, 1)
print(n)
EOF""" % (recording, first, last_post)).strip())


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
        keep = os.path.basename(KEEP)
        ssh("mkdir -p ~/%s && cd ~/%s && zstd -q -T4 -f --rm drive-%s.bin -o %s/drive-%s.bin.zst && "
            "cp drive-%s.auto %s/" % (KEEP, RUNS, name, keep, name, name, keep))
        zst = int(ssh("stat -c %%s ~/%s/drive-%s.bin.zst" % (KEEP, name)))
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
    mac, cachy = free_gb()
    log(f, "%s: free Mac %.0f GB, cachy %d GB" % (name, mac, cachy))
    if mac < MAC_FLOOR or cachy - CACHY_PAIR * reserved_pairs < CACHY_FLOOR:
        log(f, "stop: a floor would be crossed (Mac %d GB, cachy %d GB)" % (MAC_FLOOR, CACHY_FLOOR))
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
        wait_cachy_idle(f)
        numbers = "%d %d %d %d 1 0" % (m, car, spec["weather"], TYRES[surf])
        if NGP:
            rc, text = ngp_originals(f, name, numbers, env)
        else:
            venv = dict(env, RBR_TIMEOUT="1800", V1_REPEAT="1")
            rc, text = run(f, [os.path.join(REPO, "harness/v1-drive.sh"), name, numbers, name + "-ap", "999999"], venv)
        ssh("rm -f ~/%s/drive-%s-ap.playback.bin ~/%s/drive-%s.playback.bin" % (RUNS, name, RUNS, name))
        row["games_min"] = "%.1f" % ((time.time() - t1) / 60)
        repeat = dict(re.findall(r"^repeat (static|noise): (.*)$", text, re.M))
        row["ref1_eq_ref2"] = "static mask: %s; %s noise mask: %s" % (
            repeat.get("static", "no result"), "NGP drive1" if NGP else "tanner", repeat.get("noise", "no result"))
    if drove["steps"] >= MIN_STEPS and rc == 0:
        return Check(f, spec, row, end, drove["start"] + drove["steps"], csv_path)
    if drove["steps"] < MIN_STEPS:
        row.update(ref1_eq_ref2="-", rust_equal_compared="-", first_difference_or_stop="autopilot: %s" % end)
    else:
        row.update(rust_equal_compared="-", first_difference_or_stop="v1-drive.sh failed (exit %d)" % rc)
    clean(name)
    write_row(f, csv_path, row)
    f.close()
    return None


def ngp_originals(f, name, numbers, env):
    """V1_REPEAT's step 3 in the NGP test install: the autopilot's playback twice (`scripts/ngp-run.sh`), drive-<name>
    and ref-<name>-2, compared by diverge.py on the static mask, then also on NGP's noise mask (drive1.bin.mask, notes/ngp.md); ref-2 is
    deleted. Returns the exit code and the output (with v1-drive.sh's `repeat` lines)."""
    playback = "%s/drive-%s-ap.playback.bin" % (RUNS, name)
    renv = dict(env, AUTO=numbers, PLAYBACK_REMOTE=playback)
    for run_name in ("drive-" + name, "ref-%s-2" % name):
        wait_cachy_idle(f)
        rc, text = run(f, [os.path.join(REPO, "scripts/ngp-run.sh"), run_name, str(NGP_SECS)], renv)
        if rc:
            return rc, text
    subprocess.run(["scp", "-q", os.path.join(REPO, "harness/diverge.py"), "cachy:/tmp/rbr-diverge-%s.py" % name],
                   check=True)
    text = ssh("""set -u
cd ~/%(runs)s
echo "%(numbers)s" > drive-%(name)s.auto
out=$(python3 -I /tmp/rbr-diverge-%(name)s.py drive-%(name)s.bin ref-%(name)s-2.bin 2>&1) || true
echo "repeat static: ${out//$'\n'/; }"
ln -sfn ~/rbr-ngp-runs/drive1.bin.mask drive-%(name)s.bin.mask
out=$(python3 -I /tmp/rbr-diverge-%(name)s.py drive-%(name)s.bin ref-%(name)s-2.bin 2>&1) || true
echo "repeat noise: ${out//$'\n'/; }"
rm -f drive-%(name)s.bin.mask ref-%(name)s-2.bin ref-%(name)s-2.harness.log /tmp/rbr-diverge-%(name)s.py
""" % dict(runs=RUNS, name=name, numbers=numbers))
    log(f, text.strip())
    return 0, text


def parse(line):
    words = line.split()
    stage = words[0]
    if NGP and len(words) > 3:
        sys.exit("%s: --ngp starts from the stage start only (no base)" % line)
    if len(words) == 3 and words[2] != "0":
        sys.exit("%s: the boot has good weather only (no CWeatherDaemon); other weathers need a base" % line)
    m = int(re.fullmatch(r"track-(\d+)_[A-Z]", stage)[1])
    return dict(stage=stage, map=m, car=int(words[1]), weather=int(words[2]) if len(words) > 2 else 0,
                base=words[3] if len(words) > 3 else None)


def use_ngp():
    global NGP, RUNS, KEEP, PREFIX, SLOTS, STEPS, GROUP, GAME, CACHY_PAIR, DEFAULT_CSV
    NGP, RUNS, PREFIX = True, "rbr-ngp-runs", "ngp"
    KEEP = RUNS + "/ngp-matrix"
    SLOTS = (2, 4, 5, 6)  # the cars NGP's physics.rbz has
    STEPS, GROUP = 130000, 2
    CACHY_PAIR = 5  # two 130k-step recordings of 2.1 GB and the playback
    GAME = "[r]br-ngp-7.4.778/RichardBurnsRally_SSE"
    DEFAULT_CSV = os.path.join(os.path.dirname(DEFAULT_CSV), "ngp-matrix.csv")


def main():
    if "--ngp" in sys.argv:
        sys.argv.remove("--ngp")
        use_ngp()
    if sys.argv[1:2] == ["list"]:
        for m, stage in stages().items():
            for car in SLOTS:
                print(stage, car)
        return
    if len(sys.argv) < 3 or sys.argv[1] != "run":
        sys.exit(__doc__)
    csv_path = sys.argv[sys.argv.index("--csv") + 1] if "--csv" in sys.argv else DEFAULT_CSV
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

    if specs:
        wait_cachy_rust_idle()
    pool = concurrent.futures.ThreadPoolExecutor(max_workers=GROUP)

    def setup(offset, built, held):
        """Prepares the group at `offset` (`held` pairs' recordings still on cachy) and starts its autopilots.
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
            # Do not remove files an interrupted harness game could still be writing. A
            # previous Rust job was drained before entering this loop.
            wait_cachy_idle(states[0]["f"])
            for state in states:
                clean(state["name"])
            prepare_rust()
        else:
            if NGP:
                wait_cachy_idle(states[0]["f"])
            for state in states:
                clean(state["name"])
        mac, cachy = free_gb()
        if mac < MAC_FLOOR or cachy - CACHY_PAIR * (held + len(states)) < CACHY_FLOOR:
            for state in states:
                log(state["f"], "stop: a floor would be crossed (Mac %d GB, cachy %d GB)" %
                    (MAC_FLOOR, CACHY_FLOOR))
                state["f"].close()
            return None
        # These pairs share a single synced release binary, built above before the first group.
        return states, [pool.submit(run_autopilot, state) for state in states], stop_batch

    group = setup(0, False, 0) if specs else None
    offset = 0
    while group:
        states, futures, stop_batch = group
        for future in futures:
            future.result()
        offset += GROUP
        if NGP:  # the next group's autopilots run while this group's games (serial) and checks do
            group = None if stop_batch else setup(offset, True, len(states))
        checks = []
        for state in states:
            check = pair(state, csv_path, env)
            if check:
                checks.append(check)
        # Their checks finish together, in spec order, before the next group's games.
        for check in checks:
            check.done()
        if not NGP:
            group = None if stop_batch else setup(offset, True, 0)
    pool.shutdown()


if __name__ == "__main__":
    main()
