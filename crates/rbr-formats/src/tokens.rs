//! The word stream of the text files read one word at a time (`pss`, `repair`): `;` starts a comment that runs to
//! the end of the line, `$` ends the text, and words are split on white space. The reader only hands out words; each
//! module maps the missing word to its own error, keeping its messages. Number parsing stays in each module, as it
//! differs: `pss` takes the leading digits like the loader's `atoi`-style reads, `repair` parses strictly.
//!
//! `ai_ini` keeps its own line cursor: names are whole lines and its end errors carry line numbers.

/// The words of `text` up to `$`, without comments.
pub(crate) fn words(text: &str) -> Vec<&str> {
    let text = text.split('$').next().unwrap_or("");
    text.lines().flat_map(|line| line.split(';').next().unwrap_or("").split_whitespace()).collect()
}

/// The words, with the position of the next one.
pub(crate) struct Reader<'a> {
    words: Vec<&'a str>,
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(words: Vec<&'a str>) -> Self {
        Reader { words, pos: 0 }
    }

    /// The next word, or `None` at the end.
    pub(crate) fn next(&mut self) -> Option<&'a str> {
        let word = self.words.get(self.pos)?;
        self.pos += 1;
        Some(word)
    }

    pub(crate) fn peek(&self) -> Option<&'a str> {
        self.words.get(self.pos).copied()
    }

    /// The words left, for the trailing check.
    pub(crate) fn rest(&self) -> &[&'a str] {
        &self.words[self.pos..]
    }
}
