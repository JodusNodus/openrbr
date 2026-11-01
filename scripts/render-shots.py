#!/usr/bin/env python3
"""Reference shots: renders the shots of data/shots.json (1080p, every enhancement) into a folder.

    scripts/render-shots.py [--out DIR] [--only NAME...] [--jobs N] [--list] [--compare OLD_DIR] [--dry]
                            [--previews DIR [--no-render]] [--gallery FILE] [--diff DIR]
                            [--set KEY=VALUE] [--args STRING] [--cars CAR...] [--rename NEW] [--save]

Each shot is a stage, a car, a `view` (VIEWS), the clock, a launcher selection (weather), viewer arguments and the
drive frame (`at`) the autopilot has reached; the shot is one `viewer --screenshot` of a live run in lockstep, so the same build gives the
same picture. They serve as README pictures and as the reference for graphics changes: render before and after a
change, then `--compare` the folders.

Needs the release `rerbr`, `viewer` and `rbr-standalone` (`cargo build --release`), the game install (RBR_DATA, or
the launcher settings' install folder) with NGP and the RallySimFans stages and cars the shots name, and, for the
same textures as the committed list was made with, the upscaler (`Upscaler/` in the files folder). The player's
launcher-settings.ini is only read (its install folder and NGP paths); the rest comes from `SETTINGS` below, in a
temporary files folder, so a player's own options do not change a shot. Writes DIR/<name>.png, DIR/<name>.log (the
exact command) and DIR/manifest.json (build and tool versions). `--previews DIR` also writes JPEGs (at most 1920x1080) there
(ImageMagick; docs/shots/ holds the committed ones), `--no-render` only converts what --out has. `--gallery FILE`
writes the markdown page of the shots (docs/shots/README.md). `--compare OLD_DIR` prints each shot's RMSE against an
earlier render, worst first; with `--diff DIR` it also writes the difference, times 8, as a 1080p JPEG.
`--set KEY=VALUE` (repeatable; car, view, clock, stage, at, settings.<name>) changes the selected shots for the run
(`--save` writes the change back to the shots file), `--args STRING` (repeatable, shell-split) appends viewer
arguments. `--cars A B` renders each selected shot once per car (as <name>--<slug>) plus a labelled <name>--cars.jpg
sheet. `--rename NEW` renames the single `--only` shot; with `--save` and `--previews` the old JPEG is deleted.
"""
import argparse, copy, json, os, re, shlex, subprocess, sys, tempfile, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# What a shot fixes in the launcher's settings (the rest is default); visible options only.
SETTINGS = {
    'weather': 'good', 'wetness': 'dry', 'fog': 'off', 'precipitation': 'none', 'wear': 'new', 'tyre': 'auto',
    'fov': '55', 'wipers': 'auto', 'dials': 'on', 'mirrors': '1', 'anti_aliasing': 'taa',
    'wet_roads': 'high', 'render_scale': '100', 'upscale_textures': 'on', 'cabin': 'on', 'start_time': '16:00',
}
# Copied from the player's settings: where the game and NGP are.
KEEP = ('install_folder', 'ngp_physics', 'ngp_dll', 'rsf_root')
# The most the renderer draws: `all` leaves out the grass.
COMMON = ['--no-hud', '--enhance', 'all,grass', '--detail', 'ultra', '--anti-aliasing', 'taa', '--reflections', 'high',
          '--at-periods', '5']
# What a shot's `view` adds to the viewer's arguments (a fly-by is a chase view with a camera by the road).
VIEWS = {
    'chase': ['--view', 'chase'],
    'cockpit': ['--view', 'cockpit', '--fov', '65'],
    'cockpit-clean': ['--view', 'cockpit', '--fov', '65', '--no-particles'],  # no mud on the glass, no dust
    'flyby': ['--view', 'chase', '--fov', '45', '--flyby', '9,2.5,0.7'],
}
# Arguments of the launcher's command a screenshot does not want (the number of values each takes).
DROP = {'--fullscreen': 0, '--title': 1, '--ghost-weather': 1, '--present': 1, '--fps': 0,
        '--hud-hide': 1, '--size': 1, '--quit-after': 1, '--save-replay': 1}
SIZE = '1920x1080'
# Shots rendered at once: a shot's load and drive are mostly one thread, its peak memory up to 5 GB (580 at 1080p).
JOBS = 3


def files_folder(given):
    if given:
        return Path(given)
    if os.environ.get('RERBR_HOME'):
        return Path(os.environ['RERBR_HOME'])
    return Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local/share')) / 'rerbr'


def read_keys(path):
    keys, section = {}, None
    for line in path.read_text().splitlines():
        line = line.strip()
        if line.startswith('['):
            section = line
        elif section == '[launcher]' and '=' in line:
            k, v = line.split('=', 1)
            keys[k.strip()] = v.strip()
    return keys


def launcher_command(build, home, shot, size):
    """The launcher's Start command for `shot`'s selection, in a temporary files folder."""
    tmp = Path(tempfile.mkdtemp(prefix='rerbr-shots-'))
    for entry in home.iterdir():
        if entry.name != 'launcher-settings.ini':
            (tmp / entry.name).symlink_to(entry)
    keys = {k: v for k, v in read_keys(home / 'launcher-settings.ini').items() if k in KEEP}
    keys.update(SETTINGS, size=size, car=shot['car'], map=str(shot['stage']), **shot.get('settings', {}))
    (tmp / 'launcher-settings.ini').write_text('[launcher]\n' + ''.join(f'{k} = {v}\n' for k, v in keys.items()))
    done = subprocess.run([build / 'rerbr', '--command'], env={**os.environ, 'RERBR_HOME': str(tmp)},
                          capture_output=True, text=True)
    if done.returncode:
        sys.exit(f"{shot['name']}: rerbr --command: {done.stderr.strip()}")
    return tmp, done.stdout.splitlines()


def shot_command(build, lines, shot, out, size):
    """(environment, viewer command) from the launcher's lines: its variables, the viewer's arguments with the
    screenshot's changes, then the live process's, driven by the autopilot."""
    env, i = dict(os.environ), 0
    while '=' in lines[i] and not lines[i].startswith('/'):
        k, v = lines[i].split('=', 1)
        env[k], i = v, i + 1
    argv = lines[i:]
    live = argv.index('--live')
    viewer, rest = argv[:live], argv[live:]
    command, j = [str(build / 'viewer')], 1
    while j < len(viewer):
        if viewer[j] in DROP:
            j += 1 + DROP[viewer[j]]
            continue
        command.append(viewer[j])
        j += 1
    rest[1] = str(build / 'rbr-standalone')
    clock = ['--clock', shot['clock']] if 'clock' in shot else []
    command += ['--size', size, '--screenshot', str(out)] + COMMON + VIEWS[shot['view']] + clock + shot.get('args', []) + ['--at', str(shot['at'])]
    return env, command + rest + ['--pilot', 'autopilot']


def check_car(command, shot):
    norm = lambda s: ''.join(c for c in s.lower() if c.isalnum()).replace('group', 'grp')
    used = Path(command[command.index('--rsf-car') + 1]).name if '--rsf-car' in command else ''
    asked = norm(shot['car'])
    if not (asked in norm(used) or norm(used).removesuffix('ngp6').removesuffix('ngp7') in asked):
        sys.exit(f"{shot['name']}: asked for {shot['car']!r}, the launcher chose {used!r} (the car is not installed?)")


def compare(new, old, diff):
    """RMSE per shot of `new` against `old` (ImageMagick's compare), worst first; `diff` gets the amplified difference."""
    rows = []
    for png in sorted(new.glob('*.png')):
        before = old / png.name
        if not before.exists():
            rows.append((float('inf'), png.name, 'no reference'))
            continue
        done = subprocess.run(['magick', 'compare', '-metric', 'RMSE', png, before, 'null:'], capture_output=True, text=True)
        text = (done.stderr or done.stdout).strip()
        if diff and done.returncode in (0, 1):
            diff.mkdir(parents=True, exist_ok=True)
            subprocess.run(['magick', before, png, '-compose', 'difference', '-composite', '-evaluate', 'multiply', '8',
                            '-resize', '1920x1080', '-quality', '88', diff / f'{png.stem}.jpg'])
        try:
            rows.append((float(text.split()[0]), png.name, text))
        except (ValueError, IndexError):
            rows.append((float('inf'), png.name, text or 'compare failed'))
    for _, name, text in sorted(rows, reverse=True):
        print(f'{name:40} {text}')


def previews(out, dest, names):
    """JPEGs (1080p at most) of the shots' PNGs in `out`, without metadata."""
    dest.mkdir(parents=True, exist_ok=True)
    for name in names:
        done = subprocess.run(['magick', out / f'{name}.png', '-filter', 'Lanczos', '-resize', '1920x1080>', '-strip',
                               '-sampling-factor', '4:2:0', '-quality', '90', dest / f'{name}.jpg'])
        if done.returncode:
            sys.exit(f'{name}: magick failed')


def apply_set(shots, pairs):
    """Applies each --set KEY=VALUE to every shot; exits on an unknown key or a bad value."""
    for item in pairs:
        key, eq, value = item.partition('=')
        if not eq or not key:
            sys.exit(f'--set {item!r}: want KEY=VALUE')
        if key in ('car', 'clock'):
            if not value:
                sys.exit(f'--set {key}: want a non-empty value')
        elif key == 'view':
            if value not in VIEWS:
                sys.exit(f"--set view={value!r}: want one of {', '.join(sorted(VIEWS))}")
        elif key in ('stage', 'at'):
            try:
                value = int(value)
            except ValueError:
                sys.exit(f'--set {item!r}: want an integer for {key}')
        elif not key.startswith('settings.') or len(key) == len('settings.'):
            sys.exit('--set {!r}: unknown key (want car, view, clock, stage, at, settings.<name>)'.format(key))
        for s in shots:
            if key.startswith('settings.'):
                s.setdefault('settings', {})[key[len('settings.'):]] = value
            else:
                s[key] = value


def apply_args(shots, extras):
    """Appends each shell-split --args STRING to every shot's viewer arguments."""
    for item in extras:
        try:
            parts = shlex.split(item)
        except ValueError as e:
            sys.exit(f'--args {item!r}: {e}')
        for s in shots:
            s.setdefault('args', []).extend(parts)


def slug(car):
    """The car name lowercased, runs of non-alphanumerics as one `-`."""
    text = re.sub(r'[^a-z0-9]+', '-', car.lower()).strip('-')
    if not text:
        sys.exit(f'--cars {car!r}: no usable name')
    return text


def expand_cars(shots, cars):
    """One copy of each shot per car, named <name>--<slug>; (copies, [(old name, [(car, new name)])])."""
    copies, plan = [], []
    for s in shots:
        variants = []
        for car in cars:
            v = copy.deepcopy(s)
            v['car'], v['name'] = car, f"{s['name']}--{slug(car)}"
            variants.append(v)
            copies.append(v)
        plan.append((s['name'], [(v['car'], v['name']) for v in variants]))
    return copies, plan


def contact_sheet(out, name, variants):
    """<out>/<name>--cars.jpg: the variants' PNGs at 960x540 in a grid of 3 columns, labelled with the car."""
    tmp = Path(tempfile.mkdtemp(prefix='rerbr-sheet-'))
    try:
        tiles = []
        for car, variant in variants:
            png = out / f'{variant}.png'
            if not png.exists():
                continue
            tile = tmp / f'{len(tiles)}.jpg'
            done = subprocess.run(['magick', png, '-resize', '960x540!', '-fill', 'rgba(0,0,0,0.55)',
                                   '-draw', 'rectangle 0,0 960,46', '-gravity', 'NorthWest', '-pointsize', '28',
                                   '-fill', 'white', '-annotate', '+14+8', car, tile],
                                  capture_output=True, text=True)
            if done.returncode:
                sys.exit(f'{variant}: magick failed: {(done.stderr or done.stdout).strip()}')
            tiles.append(tile)
        if not tiles:
            return
        rows = []
        for i in range(0, len(tiles), 3):
            row = tmp / f'row{i // 3}.jpg'
            done = subprocess.run(['magick', *[str(t) for t in tiles[i:i + 3]], '+append', row],
                                  capture_output=True, text=True)
            if done.returncode:
                sys.exit(f'{name}: magick failed: {(done.stderr or done.stdout).strip()}')
            rows.append(row)
        done = subprocess.run(['magick', *[str(r) for r in rows], '-append', '-quality', '90', out / f'{name}--cars.jpg'],
                              capture_output=True, text=True)
        if done.returncode:
            sys.exit(f'{name}: magick failed: {(done.stderr or done.stdout).strip()}')
    finally:
        subprocess.run(['rm', '-rf', tmp])


GROUPS = [('day', 'Day'), ('golden', 'Golden hour'), ('rain', 'Rain'), ('fog', 'Fog'), ('snow', 'Snow'), ('night', 'Night')]


RETAIL = {52: 'Sipirkakim'}  # the retail stages the list uses; the Maps folder names only the RSF ones


def stage_names(install):
    """Stage number to name, from the install's Maps folders (`<number>-<Name>`)."""
    names = {}
    for folder in (Path(install) / 'Maps').glob('*-*') if install else ():
        number, name = folder.name.split('-', 1)
        if number.isdigit():
            names[int(number)] = name.replace('_', ' ').replace('-', ' ')
    return names


def gallery(shots, names, dest):
    """The markdown page of `shots` beside its JPEGs (`previews`), grouped by the name's first word."""
    lines = ['# Reference shots\n',
             f'JPEGs of the {len(shots)} shots of [`data/shots.json`](../../data/shots.json), made by',
             '`scripts/render-shots.py` (every enhancement on, the autopilot driving). Regenerate them with',
             '`scripts/render-shots.py --previews docs/shots --gallery docs/shots/README.md`; the graphics workflow is in',
             '[development.md](../development.md#reference-shots).\n']
    for prefix, title in GROUPS:
        group = [s for s in shots if s['name'].split('-')[0] == prefix]
        lines += [f'## {title}\n'] if group else []
        for s in group:
            stage = RETAIL.get(s['stage']) or names.get(s['stage'], 'stage')
            kind = 'retail' if s['stage'] in RETAIL else 'RSF'
            clock = f", {s['clock']}" if 'clock' in s else ''
            lines.append(f"### {s['name']}\n\n{s['car']}, {stage} ({kind} {s['stage']}), {s['view'].split('-')[0]} view{clock}\n\n"
                         f"![{s['name']}]({s['name']}.jpg)\n")
    dest.write_text('\n'.join(lines))


def tool_versions(build):
    rev = subprocess.run(['git', '-C', ROOT, 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()
    dirty = bool(subprocess.run(['git', '-C', ROOT, 'status', '--porcelain'], capture_output=True, text=True).stdout.strip())
    return {'commit': rev, 'dirty': dirty,
            'built': {p: time.ctime((build / p).stat().st_mtime) for p in ('viewer', 'rbr-standalone', 'rerbr')}}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--shots', default=ROOT / 'data/shots.json', type=Path)
    ap.add_argument('--out', default=ROOT / 'target/shots', type=Path)
    ap.add_argument('--build', default=ROOT / 'target/release', type=Path, help='folder with rerbr, viewer, rbr-standalone')
    ap.add_argument('--home', help="the player's files folder (default $RERBR_HOME or ~/.local/share/rerbr)")
    ap.add_argument('--size', default=SIZE, help='WIDTHxHEIGHT (3840x2160 for marketing renders)')
    ap.add_argument('--only', nargs='*', default=[], metavar='NAME')
    ap.add_argument('--jobs', type=int, default=JOBS, help=f'shots rendered at once (default {JOBS}; each takes up to 5 GB)')
    ap.add_argument('--list', action='store_true')
    ap.add_argument('--dry', action='store_true', help='print each command instead of running it')
    ap.add_argument('--previews', type=Path, metavar='DIR', help='also write JPEGs of the shots to DIR')
    ap.add_argument('--no-render', action='store_true', help='convert the PNGs already in --out (--previews) instead of rendering')
    ap.add_argument('--gallery', type=Path, metavar='FILE', help='write the markdown gallery of the shots to FILE')
    ap.add_argument('--diff', type=Path, metavar='DIR', help='with --compare: write the amplified difference images to DIR')
    ap.add_argument('--compare', type=Path, metavar='OLD_DIR', help='compare --out with an earlier render and exit')
    ap.add_argument('--set', action='append', default=[], metavar='KEY=VALUE',
                    help='change a shot field (car, view, clock, stage, at, settings.<name>); repeatable')
    ap.add_argument('--args', action='append', default=[], metavar='STRING', help='extra viewer arguments, shell-split; repeatable')
    ap.add_argument('--cars', nargs='+', default=[], metavar='CAR', help='render each shot once per car, plus a <name>--cars.jpg sheet')
    ap.add_argument('--rename', metavar='NEW', help='rename the single --only shot')
    ap.add_argument('--save', action='store_true', help='write the --set/--rename changes back to --shots')
    a = ap.parse_args()
    all_shots = json.loads(a.shots.read_text())
    shots = all_shots
    if a.only:
        unknown = set(a.only) - {s['name'] for s in shots}
        if unknown:
            sys.exit(f'unknown shots: {", ".join(sorted(unknown))}')
        shots = [s for s in shots if s['name'] in a.only]
    apply_set(shots, a.set)
    renamed_from = None
    if a.rename:
        if len(a.only) != 1 or len(shots) != 1:
            sys.exit('--rename needs exactly one --only shot')
        if a.rename != shots[0]['name'] and a.rename in {s['name'] for s in all_shots}:
            sys.exit(f'--rename {a.rename!r}: a shot with that name already exists')
        renamed_from, shots[0]['name'] = shots[0]['name'], a.rename
    if a.save and a.cars:
        sys.exit('--save cannot be used with --cars')
    saved = json.dumps(all_shots, indent=1) + '\n'
    # The run's own copies: `--args` and `--cars` are not saved.
    run = copy.deepcopy(shots)
    apply_args(run, a.args)
    render_shots, plan = expand_cars(run, a.cars) if a.cars else (run, [])
    if a.list:
        for s in shots:
            print(f"{s['name']:36} stage {s['stage']:>3}  {s['car']}")
        return
    if a.compare:
        return compare(a.out, a.compare, a.diff)
    home = files_folder(a.home)
    write_gallery = lambda: a.gallery and gallery(
        all_shots if a.save else shots, stage_names(read_keys(home / 'launcher-settings.ini').get('install_folder')), a.gallery)
    if not a.save:
        write_gallery()
    if a.no_render:
        names = [s['name'] for s in render_shots]
    else:
        names = render(a, home, render_shots)
    if a.previews:
        previews(a.out, a.previews, names)
    for old_name, variants in plan:
        contact_sheet(a.out, old_name, variants)
    if not a.dry and not a.no_render:
        missing = [s['name'] for s in render_shots if s['name'] not in set(names)]
        if missing:
            sys.exit(f"failed shots: {', '.join(missing)}" + (' (nothing saved)' if a.save else ''))
    # Saved only once the shots rendered: the shots file, the gallery, and the renamed shot's old preview gone.
    if a.save and not a.dry:
        a.shots.write_text(saved)
        write_gallery()
        if renamed_from and a.previews:
            (a.previews / f'{renamed_from}.jpg').unlink(missing_ok=True)


def render(a, home, shots):
    """Renders `shots` into --out, --jobs at a time; the names of those that rendered."""
    a.out.mkdir(parents=True, exist_ok=True)
    manifest = {'tools': tool_versions(a.build), 'size': a.size, 'shots': {}}

    def one(shot):
        png = a.out / f"{shot['name']}.png"
        tmp, lines = launcher_command(a.build, home, shot, a.size)
        try:
            env, command = shot_command(a.build, lines, shot, png, a.size)
            try:
                check_car(command, shot)
            except SystemExit as refused:
                print(refused.code, file=sys.stderr)
                return {'exit': 1, 'command': command}
            if a.dry:
                print(' '.join(map(repr, command)))
                return None
            began = time.time()
            done = subprocess.run(command, env=env, capture_output=True, text=True)
            png.with_suffix('.log').write_text(' '.join(map(repr, command)) + '\n\n' + done.stderr + done.stdout)
            print(f"{shot['name']}: {'ok' if done.returncode == 0 else 'FAILED ' + str(done.returncode)} "
                  f"in {time.time() - began:.0f} s", flush=True)
            if done.returncode:
                print(done.stderr[-800:], file=sys.stderr)
            return {'exit': done.returncode, 'command': command}
        finally:
            subprocess.run(['rm', '-rf', tmp])

    with ThreadPoolExecutor(1 if a.dry else a.jobs) as pool:
        for shot, result in zip(shots, pool.map(one, shots)):
            if result:
                manifest['shots'][shot['name']] = result
    if not a.dry:
        (a.out / 'manifest.json').write_text(json.dumps(manifest, indent=1) + '\n')
    return [n for n, r in manifest['shots'].items() if r['exit'] == 0]


if __name__ == '__main__':
    main()
