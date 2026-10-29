//! Reader for `misc/times.ini` (in `Root/misc.rbz`): the default reference times for each stage, which the quick race
//! compares the driver's stage with. The loader is `E60_5680e0` (0x5680e0, `src/rbr/RBR_E60_Times.cpp`); the time
//! token is `E57Tokenizer::Parse` (0x55b860, `src/rbr/RBR_E57_DashGS.cpp`).
//!
//! Layout: the first token is the number of stages. Each stage is a token with its number (the `NN` of `track-NN_X`)
//! and three times: the two split times and the finish time. A time is `mm:ss:cc` (minutes, seconds, hundredths) and
//! reads as `mm * 60 + cc * 0.01 + ss` seconds in f32. `;` starts a comment that runs to the end of the line, and the
//! file ends with `$`. `misc/rbctimes.ini` (Richard Burns Challenge) is not read here.

use std::fmt::{self, Display, Formatter};
use std::path::Path;

use crate::rbz;

/// The split times in each stage's block (`CheckPoint 1` and `CheckPoint 2`).
pub const SPLITS: usize = 2;

/// One stage's reference times, in seconds from the start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageTimes {
    /// The stage number, the `NN` of `track-NN_X`.
    pub stage: u32,
    /// The split times, in order.
    pub splits: [f32; SPLITS],
    /// The finish time.
    pub finish: f32,
}

/// Failure to read `times.ini`.
#[derive(Debug)]
pub enum Error {
    /// The archive or its `Misc/times.ini` entry could not be read.
    Archive(String),
    /// The first token is not a stage count.
    BadCount,
    /// A token where a stage number or a time was expected.
    Unexpected { token: String },
    /// The input ends before the stages the count declares.
    StagesShort { declared: usize, found: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Archive(e) => write!(f, "times.ini: {e}"),
            Error::BadCount => write!(f, "times.ini: first token is not a stage count"),
            Error::Unexpected { token } => write!(f, "times.ini: unexpected token {token:?}"),
            Error::StagesShort { declared, found } => {
                write!(f, "times.ini: {declared} stages declared, {found} read")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Splits the text into whitespace-separated tokens, dropping `;` comments and the `$` end.
fn tokens(text: &str) -> Vec<&str> {
    text.lines()
        .flat_map(|line| line.split(';').next().unwrap_or("").split_whitespace())
        .take_while(|t| *t != "$")
        .collect()
}

/// One time token (`E57Tokenizer::Parse`): `mm:ss:cc` as `mm * 60 + cc * 0.01 + ss` in f32, in the original's order of
/// operations. Any other token is read as a plain number.
pub fn parse_time(token: &str) -> Option<f32> {
    let fields: Vec<&str> = token.split(':').collect();
    match fields[..] {
        [minutes, seconds, hundredths] => {
            let (minutes, seconds, hundredths) =
                (minutes.parse::<f32>().ok()?, seconds.parse::<f32>().ok()?, hundredths.parse::<f32>().ok()?);
            Some(minutes * 60.0 + hundredths * 0.01 + seconds)
        }
        _ => token.parse().ok(),
    }
}

/// Parses `times.ini`: the stages in file order. Tokens after the declared stages are ignored, as the loader ignores them.
pub fn parse(text: &str) -> Result<Vec<StageTimes>, Error> {
    let mut rest = tokens(text).into_iter();
    let declared: usize = rest.next().and_then(|t| t.parse().ok()).ok_or(Error::BadCount)?;
    // No reservation: `declared` comes from the file; the loop below fails at the first missing stage.
    let mut stages = Vec::new();
    while stages.len() < declared {
        let found = stages.len();
        let short = || Error::StagesShort { declared, found };
        let stage = rest.next().ok_or_else(short)?;
        let stage = stage.parse().map_err(|_| Error::Unexpected { token: stage.to_owned() })?;
        let mut times = [0.0; SPLITS + 1];
        for time in &mut times {
            let token = rest.next().ok_or_else(short)?;
            *time = parse_time(token).ok_or_else(|| Error::Unexpected { token: token.to_owned() })?;
        }
        stages.push(StageTimes { stage, splits: std::array::from_fn(|i| times[i]), finish: times[SPLITS] });
    }
    Ok(stages)
}

/// Reads `Misc/times.ini` from the `misc.rbz` archive at `path` (`Root/misc.rbz` in the install).
pub fn read(path: &Path) -> Result<Vec<StageTimes>, Error> {
    let mut archive = rbz::open(path).map_err(|e| Error::Archive(format!("{}: {e}", path.display())))?;
    let mut entry = archive.by_name("Misc/times.ini").map_err(|e| Error::Archive(e.to_string()))?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text).map_err(|e| Error::Archive(e.to_string()))?;
    parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_reads_minutes_seconds_and_hundredths() {
        assert_eq!(parse_time("01:37:00"), Some(97.0));
        assert_eq!(parse_time("00:57:50"), Some(57.5));
        assert_eq!(parse_time("03:05:00"), Some(185.0));
        assert_eq!(parse_time("12.5"), Some(12.5), "a plain number");
        assert_eq!(parse_time("1:x:00"), None);
    }

    #[test]
    fn parses_the_stage_blocks_in_file_order() {
        let text = "; header\n2 ; Number of stage\n;\n10 \t; Stage Number\n01:37:00 \t; CheckPoint 1 Time\n03:14:00\t; CP 2\n04:15:00\t; Finish\n;\n54 ; Stage Number\n00:57:00 \n01:59:00\n03:05:00\n$\n";
        let stages = parse(text).expect("parse");
        assert_eq!(
            stages,
            [
                StageTimes { stage: 10, splits: [97.0, 194.0], finish: 255.0 },
                StageTimes { stage: 54, splits: [57.0, 119.0], finish: 185.0 },
            ]
        );
    }

    #[test]
    fn the_end_marker_and_later_tokens_are_not_read() {
        let stages = parse("1\n5\n00:01:00 00:02:00 00:03:00\n$\n7 7 7 7").expect("parse");
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].finish, 3.0);
    }

    #[test]
    fn a_huge_declared_count_is_an_error() {
        // Billions of stages would be a huge allocation up front; the file ends after one stage.
        let text = "4294967295\n5\n00:01:00 00:02:00 00:03:00\n$\n";
        assert!(matches!(parse(text), Err(Error::StagesShort { .. })));
    }

    #[test]
    fn short_or_bad_files_are_errors() {
        assert!(matches!(parse("x\n"), Err(Error::BadCount)));
        assert!(matches!(parse("2\n5 1:00:00 1:00:00 1:00:00\n"), Err(Error::StagesShort { declared: 2, found: 1 })));
        assert!(matches!(parse("1\n5 1:00:00 zz 1:00:00\n"), Err(Error::Unexpected { .. })));
    }
}
