//! Watching a replay (`--replay FILE`): the file's header holds the viewer arguments that drove
//! (`rbr_recording::replay`), its frames the drive's last start, run again by the stepping process.

use std::path::Path;

/// The viewer arguments that watch the replay read from `path` (its header's arguments, `header`): the header's
/// viewer arguments, the current command line's extra arguments over them (so they override, for example `--view`),
/// then `--live` with the header's stepping command serving `path`. The header's viewer arguments lose
/// `--save-replay`, `--save-ghost` and `--best-ghost` (each with its value), and the stepping command loses
/// `--frames-out`, `--replay-out` and `--pilot` (each with its value). The current command line keeps its own
/// `--replay FILE`: the viewer's marker that it watches.
pub fn expand(header: &[String], current: &[String], path: &str) -> Vec<String> {
    let live = header.iter().position(|a| a == "--live").unwrap_or(header.len());
    let (viewer, command) = (header[..live].to_vec(), header.get(live + 1..).unwrap_or(&[]).to_vec());
    let drop = |args: Vec<String>, flags: &[&str]| {
        let mut out = Vec::with_capacity(args.len());
        let mut skip = false;
        for arg in args {
            if skip {
                skip = false;
            } else if flags.contains(&arg.as_str()) {
                skip = true;
            } else {
                out.push(arg);
            }
        }
        out
    };
    let viewer = drop(viewer, &["--save-replay", "--save-ghost", "--best-ghost"]);
    let command = drop(command, &["--frames-out", "--replay-out", "--pilot"]);
    viewer
        .into_iter()
        .chain(current.to_vec())
        .chain(["--live".to_owned()])
        .chain(command)
        .chain(["--replay".to_owned(), path.to_owned()])
        .collect()
}

/// Writes the replay `path`'s header for a drive with `argv` (the viewer's arguments as given, without
/// `--save-replay FILE`): its parent folder made first.
pub fn record(path: &Path, argv: &[String]) -> std::io::Result<()> {
    let mut args = Vec::with_capacity(argv.len());
    let mut skip = false;
    for arg in argv {
        if skip {
            skip = false;
        } else if arg == "--save-replay" {
            skip = true;
        } else {
            args.push(arg.clone());
        }
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, rbr_recording::replay::header(&args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn replay_runs_the_headers_command_with_the_current_viewer_arguments() {
        let header = [
            "track-54_E",
            "--car",
            "COROLLA",
            "--save-replay",
            "/r/old.rrp",
            "--save-ghost",
            "g.gho",
            "--best-ghost",
            "b.ghc",
            "--ghost",
            "r.gho",
            "--live",
            "/b/rbr-standalone",
            "--live",
            "--boot",
            "--pilot",
            "autopilot",
            "--frames-out",
            "f.rtf",
            "--stage",
            "track-54_E",
        ]
        .map(str::to_owned);
        let current = ["--replay", "/r/a.rrp", "--view", "cockpit"].map(str::to_owned);
        assert_eq!(
            expand(&header, &current, "/r/a.rrp"),
            [
                "track-54_E",
                "--car",
                "COROLLA",
                "--ghost",
                "r.gho",
                "--replay",
                "/r/a.rrp",
                "--view",
                "cockpit",
                "--live",
                "/b/rbr-standalone",
                "--live",
                "--boot",
                "--stage",
                "track-54_E",
                "--replay",
                "/r/a.rrp",
            ]
            .map(str::to_owned)
            .to_vec()
        );
    }

    #[test]
    fn the_expanded_watch_parses_with_replay_on_both_sides() {
        let header = ["track-54_E", "--car", "COROLLA", "--live", "/b/rbr-standalone", "--live"].map(str::to_owned);
        let current = ["--replay", "/r/a.rrp", "--view", "cockpit"].map(str::to_owned);
        let args = crate::args::parse_args(expand(&header, &current, "/r/a.rrp").into_iter()).unwrap();
        assert_eq!(args.replay, Some(PathBuf::from("/r/a.rrp")));
        assert_eq!(args.view.map(|v| v.name().to_owned()).as_deref(), Some("cockpit"));
        assert_eq!(args.live.unwrap()[2..], ["--replay".to_owned(), "/r/a.rrp".to_owned()]);
        let args = crate::args::parse_args(
            ["--save-replay", "/r/a.rrp", "--live", "/b/rbr-standalone"].map(str::to_owned).into_iter(),
        )
        .unwrap();
        assert_eq!(args.save_replay, Some(PathBuf::from("/r/a.rrp")));
    }
}
