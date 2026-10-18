//! Case folding of the literal characters, under case insensitivity
//! (`crates/probe/cases/regexp-folding.txt`, `regexp-operator.txt`). ICU
//! compares a run of literal characters by the full folding of their case:
//! `ss` matches `ß`, `ß` matches `ss`, `ffi` matches `ﬃ`, `ﬀi` and `FFI`. A
//! character alone, or in a class, matches the characters of its own case,
//! as in PCRE2, except three pairs that only ICU matches with each other
//! (`APART`). PCRE2 compares each character with its case only.
//!
//! Only the foldings of a character into several ones (`F` of
//! `CaseFolding.txt`, `MULTI`) tell the two libraries apart: the folded run
//! holds them where a character of the pattern folds into several ones, or
//! where several characters fold into one of them. Each piece of the run
//! that such a folding crosses has its paths: the ways to fold a string into
//! it, a character after another or several at once. The tables come from
//! Unicode 15.1 (`casefold` of Python); ICU and PCRE2 have each their own
//! version of Unicode, a limit of the translation (`docs/DEV-DIALECTS.md`,
//! section 6.3).

/// Folding of a character into several ones, and the characters that fold
/// so.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Multi {
    /// The folding.
    pub folding: &'static str,
    /// Characters folded into it: one, or two of different cases.
    pub chars: &'static [char],
}

impl Multi {
    /// Folding `folding` of `chars`.
    const fn new(folding: &'static str, chars: &'static [char]) -> Self {
        Self { folding, chars }
    }

    /// Tells whether PCRE2 matches the characters of the folding with each
    /// other under case insensitivity: `ß` and `ẞ`, not `ﬅ` and `ﬆ`.
    pub fn paired(&self) -> bool {
        self.chars.len() == 2 && !apart(self.chars[0])
    }
}

/// Step of a path: a character of the folded run, or several at once by a
/// character that folds into them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// A character, matched with the characters of its case.
    Char(char),
    /// The characters of a folding of several ones.
    Multi(&'static Multi),
}

/// Run of literal characters folded.
#[derive(Debug)]
pub(super) struct Folded {
    /// The characters folded.
    pub chars: Vec<char>,
    /// Position in `chars` of the folding of each character of the run,
    /// then of its end.
    pub bounds: Vec<usize>,
    /// Foldings of several characters found in `chars`: start, end and
    /// folding, by their start.
    pub spans: Vec<(usize, usize, &'static Multi)>,
}

impl Folded {
    /// Pieces of the folded run, by their start and end in `chars`: the
    /// positions that no folding of several characters crosses cut it.
    pub fn pieces(&self) -> Vec<(usize, usize)> {
        let mut pieces = Vec::new();
        let mut start = 0;
        for position in 1..=self.chars.len() {
            if !self
                .spans
                .iter()
                .any(|&(from, to, _)| from < position && position < to)
            {
                pieces.push((start, position));
                start = position;
            }
        }
        pieces
    }

    /// Paths of the piece `start..end`; `None` beyond `limit` paths.
    pub fn paths(&self, start: usize, end: usize, limit: usize) -> Option<Vec<Vec<Step>>> {
        let mut paths = Vec::new();
        let mut path = Vec::new();
        self.walk(start, end, limit, &mut path, &mut paths)
            .then_some(paths)
    }

    /// Adds to `paths` the paths from `at` to `end` after `path`; `false`
    /// beyond `limit` paths.
    fn walk(
        &self,
        at: usize,
        end: usize,
        limit: usize,
        path: &mut Vec<Step>,
        paths: &mut Vec<Vec<Step>>,
    ) -> bool {
        if at == end {
            paths.push(path.clone());
            return paths.len() <= limit;
        }
        let steps = std::iter::once((Step::Char(self.chars[at]), at + 1)).chain(
            self.spans
                .iter()
                .filter(|&&(from, to, _)| from == at && to <= end)
                .map(|&(_, to, multi)| (Step::Multi(multi), to)),
        );
        for (step, next) in steps {
            path.push(step);
            let within = self.walk(next, end, limit, path, paths);
            path.pop();
            if !within {
                return false;
            }
        }
        true
    }
}

/// Folds `run`, its characters in order.
pub(super) fn fold(run: &[char]) -> Folded {
    let mut chars = Vec::with_capacity(run.len());
    let mut bounds = vec![0];
    for &c in run {
        match multi(c) {
            Some(multi) => chars.extend(multi.folding.chars()),
            None => chars.push(fold_char(c)),
        }
        bounds.push(chars.len());
    }
    let mut spans = Vec::new();
    for multi in &MULTI {
        let folding: Vec<char> = multi.folding.chars().collect();
        for start in 0..chars.len() {
            if chars[start..].starts_with(&folding) {
                spans.push((start, start + folding.len(), multi));
            }
        }
    }
    spans.sort_by_key(|&(start, end, _)| (start, end));
    Folded {
        chars,
        bounds,
        spans,
    }
}

/// Folding of several characters of `c`.
pub(super) fn multi(c: char) -> Option<&'static Multi> {
    MULTI.iter().find(|multi| multi.chars.contains(&c))
}

/// Tells whether `c` is a character of `APART`.
pub(super) fn apart(c: char) -> bool {
    APART.iter().any(|&(a, b)| a == c || b == c)
}

/// Character of another case than `c` that folds into it, an uppercase one
/// first: `S` for `s`, `Ι` for `ι`.
pub(super) fn partner(c: char) -> Option<char> {
    let mut partners = FOLD.iter().filter(|&&(_, folded)| folded == c);
    let first = partners.clone().next()?.0;
    Some(
        partners
            .find(|(other, _)| other.is_uppercase())
            .map_or(first, |&(other, _)| other),
    )
}

/// Folding of `c` into one character, itself when no folding of several
/// characters holds the folding.
fn fold_char(c: char) -> char {
    FOLD.iter()
        .find(|&&(other, _)| other == c)
        .map_or(c, |&(_, folded)| folded)
}

/// Pairs of characters of the same folding that ICU matches with each other
/// under case insensitivity, alone or in a class, PCRE2 not
/// (`crates/probe/cases/regexp-folding.txt`).
pub(super) const APART: [(char, char); 3] = [
    ('\u{390}', '\u{1FD3}'),
    ('\u{3B0}', '\u{1FE3}'),
    ('\u{FB05}', '\u{FB06}'),
];

/// Foldings of a character into several ones, by their folding.
const MULTI: [Multi; 73] = [
    // latin small letter a with right half ring
    Multi::new("a\u{2BE}", &['\u{1E9A}']),
    // latin small ligature ff
    Multi::new("ff", &['\u{FB00}']),
    // latin small ligature ffi
    Multi::new("ffi", &['\u{FB03}']),
    // latin small ligature ffl
    Multi::new("ffl", &['\u{FB04}']),
    // latin small ligature fi
    Multi::new("fi", &['\u{FB01}']),
    // latin small ligature fl
    Multi::new("fl", &['\u{FB02}']),
    // latin small letter h with line below
    Multi::new("h\u{331}", &['\u{1E96}']),
    // latin capital letter i with dot above
    Multi::new("i\u{307}", &['\u{130}']),
    // latin small letter j with caron
    Multi::new("j\u{30C}", &['\u{1F0}']),
    // latin small letter sharp s, and another character
    Multi::new("ss", &['\u{DF}', '\u{1E9E}']),
    // latin small ligature long s t, and another character
    Multi::new("st", &['\u{FB05}', '\u{FB06}']),
    // latin small letter t with diaeresis
    Multi::new("t\u{308}", &['\u{1E97}']),
    // latin small letter w with ring above
    Multi::new("w\u{30A}", &['\u{1E98}']),
    // latin small letter y with ring above
    Multi::new("y\u{30A}", &['\u{1E99}']),
    // latin small letter n preceded by apostrophe
    Multi::new("\u{2BC}n", &['\u{149}']),
    // greek small letter alpha with oxia and ypogegrammeni
    Multi::new("\u{3AC}\u{3B9}", &['\u{1FB4}']),
    // greek small letter eta with oxia and ypogegrammeni
    Multi::new("\u{3AE}\u{3B9}", &['\u{1FC4}']),
    // greek small letter alpha with perispomeni
    Multi::new("\u{3B1}\u{342}", &['\u{1FB6}']),
    // greek small letter alpha with perispomeni and ypogegrammeni
    Multi::new("\u{3B1}\u{342}\u{3B9}", &['\u{1FB7}']),
    // greek small letter alpha with ypogegrammeni, and another character
    Multi::new("\u{3B1}\u{3B9}", &['\u{1FB3}', '\u{1FBC}']),
    // greek small letter eta with perispomeni
    Multi::new("\u{3B7}\u{342}", &['\u{1FC6}']),
    // greek small letter eta with perispomeni and ypogegrammeni
    Multi::new("\u{3B7}\u{342}\u{3B9}", &['\u{1FC7}']),
    // greek small letter eta with ypogegrammeni, and another character
    Multi::new("\u{3B7}\u{3B9}", &['\u{1FC3}', '\u{1FCC}']),
    // greek small letter iota with dialytika and varia
    Multi::new("\u{3B9}\u{308}\u{300}", &['\u{1FD2}']),
    // greek small letter iota with dialytika and tonos, and another character
    Multi::new("\u{3B9}\u{308}\u{301}", &['\u{390}', '\u{1FD3}']),
    // greek small letter iota with dialytika and perispomeni
    Multi::new("\u{3B9}\u{308}\u{342}", &['\u{1FD7}']),
    // greek small letter iota with perispomeni
    Multi::new("\u{3B9}\u{342}", &['\u{1FD6}']),
    // greek small letter rho with psili
    Multi::new("\u{3C1}\u{313}", &['\u{1FE4}']),
    // greek small letter upsilon with dialytika and varia
    Multi::new("\u{3C5}\u{308}\u{300}", &['\u{1FE2}']),
    // greek small letter upsilon with dialytika and tonos, and another character
    Multi::new("\u{3C5}\u{308}\u{301}", &['\u{3B0}', '\u{1FE3}']),
    // greek small letter upsilon with dialytika and perispomeni
    Multi::new("\u{3C5}\u{308}\u{342}", &['\u{1FE7}']),
    // greek small letter upsilon with psili
    Multi::new("\u{3C5}\u{313}", &['\u{1F50}']),
    // greek small letter upsilon with psili and varia
    Multi::new("\u{3C5}\u{313}\u{300}", &['\u{1F52}']),
    // greek small letter upsilon with psili and oxia
    Multi::new("\u{3C5}\u{313}\u{301}", &['\u{1F54}']),
    // greek small letter upsilon with psili and perispomeni
    Multi::new("\u{3C5}\u{313}\u{342}", &['\u{1F56}']),
    // greek small letter upsilon with perispomeni
    Multi::new("\u{3C5}\u{342}", &['\u{1FE6}']),
    // greek small letter omega with perispomeni
    Multi::new("\u{3C9}\u{342}", &['\u{1FF6}']),
    // greek small letter omega with perispomeni and ypogegrammeni
    Multi::new("\u{3C9}\u{342}\u{3B9}", &['\u{1FF7}']),
    // greek small letter omega with ypogegrammeni, and another character
    Multi::new("\u{3C9}\u{3B9}", &['\u{1FF3}', '\u{1FFC}']),
    // greek small letter omega with oxia and ypogegrammeni
    Multi::new("\u{3CE}\u{3B9}", &['\u{1FF4}']),
    // armenian small ligature ech yiwn
    Multi::new("\u{565}\u{582}", &['\u{587}']),
    // armenian small ligature men ech
    Multi::new("\u{574}\u{565}", &['\u{FB14}']),
    // armenian small ligature men ini
    Multi::new("\u{574}\u{56B}", &['\u{FB15}']),
    // armenian small ligature men xeh
    Multi::new("\u{574}\u{56D}", &['\u{FB17}']),
    // armenian small ligature men now
    Multi::new("\u{574}\u{576}", &['\u{FB13}']),
    // armenian small ligature vew now
    Multi::new("\u{57E}\u{576}", &['\u{FB16}']),
    // greek small letter alpha with psili and ypogegrammeni, and another character
    Multi::new("\u{1F00}\u{3B9}", &['\u{1F80}', '\u{1F88}']),
    // greek small letter alpha with dasia and ypogegrammeni, and another character
    Multi::new("\u{1F01}\u{3B9}", &['\u{1F81}', '\u{1F89}']),
    // greek small letter alpha with psili and varia and ypogegrammeni, and another character
    Multi::new("\u{1F02}\u{3B9}", &['\u{1F82}', '\u{1F8A}']),
    // greek small letter alpha with dasia and varia and ypogegrammeni, and another character
    Multi::new("\u{1F03}\u{3B9}", &['\u{1F83}', '\u{1F8B}']),
    // greek small letter alpha with psili and oxia and ypogegrammeni, and another character
    Multi::new("\u{1F04}\u{3B9}", &['\u{1F84}', '\u{1F8C}']),
    // greek small letter alpha with dasia and oxia and ypogegrammeni, and another character
    Multi::new("\u{1F05}\u{3B9}", &['\u{1F85}', '\u{1F8D}']),
    // greek small letter alpha with psili and perispomeni and ypogegrammeni, and another character
    Multi::new("\u{1F06}\u{3B9}", &['\u{1F86}', '\u{1F8E}']),
    // greek small letter alpha with dasia and perispomeni and ypogegrammeni, and another character
    Multi::new("\u{1F07}\u{3B9}", &['\u{1F87}', '\u{1F8F}']),
    // greek small letter eta with psili and ypogegrammeni, and another character
    Multi::new("\u{1F20}\u{3B9}", &['\u{1F90}', '\u{1F98}']),
    // greek small letter eta with dasia and ypogegrammeni, and another character
    Multi::new("\u{1F21}\u{3B9}", &['\u{1F91}', '\u{1F99}']),
    // greek small letter eta with psili and varia and ypogegrammeni, and another character
    Multi::new("\u{1F22}\u{3B9}", &['\u{1F92}', '\u{1F9A}']),
    // greek small letter eta with dasia and varia and ypogegrammeni, and another character
    Multi::new("\u{1F23}\u{3B9}", &['\u{1F93}', '\u{1F9B}']),
    // greek small letter eta with psili and oxia and ypogegrammeni, and another character
    Multi::new("\u{1F24}\u{3B9}", &['\u{1F94}', '\u{1F9C}']),
    // greek small letter eta with dasia and oxia and ypogegrammeni, and another character
    Multi::new("\u{1F25}\u{3B9}", &['\u{1F95}', '\u{1F9D}']),
    // greek small letter eta with psili and perispomeni and ypogegrammeni, and another character
    Multi::new("\u{1F26}\u{3B9}", &['\u{1F96}', '\u{1F9E}']),
    // greek small letter eta with dasia and perispomeni and ypogegrammeni, and another character
    Multi::new("\u{1F27}\u{3B9}", &['\u{1F97}', '\u{1F9F}']),
    // greek small letter omega with psili and ypogegrammeni, and another character
    Multi::new("\u{1F60}\u{3B9}", &['\u{1FA0}', '\u{1FA8}']),
    // greek small letter omega with dasia and ypogegrammeni, and another character
    Multi::new("\u{1F61}\u{3B9}", &['\u{1FA1}', '\u{1FA9}']),
    // greek small letter omega with psili and varia and ypogegrammeni, and another character
    Multi::new("\u{1F62}\u{3B9}", &['\u{1FA2}', '\u{1FAA}']),
    // greek small letter omega with dasia and varia and ypogegrammeni, and another character
    Multi::new("\u{1F63}\u{3B9}", &['\u{1FA3}', '\u{1FAB}']),
    // greek small letter omega with psili and oxia and ypogegrammeni, and another character
    Multi::new("\u{1F64}\u{3B9}", &['\u{1FA4}', '\u{1FAC}']),
    // greek small letter omega with dasia and oxia and ypogegrammeni, and another character
    Multi::new("\u{1F65}\u{3B9}", &['\u{1FA5}', '\u{1FAD}']),
    // greek small letter omega with psili and perispomeni and ypogegrammeni, and another character
    Multi::new("\u{1F66}\u{3B9}", &['\u{1FA6}', '\u{1FAE}']),
    // greek small letter omega with dasia and perispomeni and ypogegrammeni, and another character
    Multi::new("\u{1F67}\u{3B9}", &['\u{1FA7}', '\u{1FAF}']),
    // greek small letter alpha with varia and ypogegrammeni
    Multi::new("\u{1F70}\u{3B9}", &['\u{1FB2}']),
    // greek small letter eta with varia and ypogegrammeni
    Multi::new("\u{1F74}\u{3B9}", &['\u{1FC2}']),
    // greek small letter omega with varia and ypogegrammeni
    Multi::new("\u{1F7C}\u{3B9}", &['\u{1FF2}']),
];

/// Characters that fold into a character of a folding of `MULTI`, other
/// than itself.
const FOLD: [(char, char); 59] = [
    ('A', 'a'),
    ('F', 'f'),
    ('H', 'h'),
    ('I', 'i'),
    ('J', 'j'),
    ('L', 'l'),
    ('N', 'n'),
    ('S', 's'),
    ('T', 't'),
    ('W', 'w'),
    ('Y', 'y'),
    ('\u{17F}', 's'),
    ('\u{345}', '\u{3B9}'),
    ('\u{386}', '\u{3AC}'),
    ('\u{389}', '\u{3AE}'),
    ('\u{38F}', '\u{3CE}'),
    ('\u{391}', '\u{3B1}'),
    ('\u{397}', '\u{3B7}'),
    ('\u{399}', '\u{3B9}'),
    ('\u{3A1}', '\u{3C1}'),
    ('\u{3A5}', '\u{3C5}'),
    ('\u{3A9}', '\u{3C9}'),
    ('\u{3F1}', '\u{3C1}'),
    ('\u{535}', '\u{565}'),
    ('\u{53B}', '\u{56B}'),
    ('\u{53D}', '\u{56D}'),
    ('\u{544}', '\u{574}'),
    ('\u{546}', '\u{576}'),
    ('\u{54E}', '\u{57E}'),
    ('\u{552}', '\u{582}'),
    ('\u{1F08}', '\u{1F00}'),
    ('\u{1F09}', '\u{1F01}'),
    ('\u{1F0A}', '\u{1F02}'),
    ('\u{1F0B}', '\u{1F03}'),
    ('\u{1F0C}', '\u{1F04}'),
    ('\u{1F0D}', '\u{1F05}'),
    ('\u{1F0E}', '\u{1F06}'),
    ('\u{1F0F}', '\u{1F07}'),
    ('\u{1F28}', '\u{1F20}'),
    ('\u{1F29}', '\u{1F21}'),
    ('\u{1F2A}', '\u{1F22}'),
    ('\u{1F2B}', '\u{1F23}'),
    ('\u{1F2C}', '\u{1F24}'),
    ('\u{1F2D}', '\u{1F25}'),
    ('\u{1F2E}', '\u{1F26}'),
    ('\u{1F2F}', '\u{1F27}'),
    ('\u{1F68}', '\u{1F60}'),
    ('\u{1F69}', '\u{1F61}'),
    ('\u{1F6A}', '\u{1F62}'),
    ('\u{1F6B}', '\u{1F63}'),
    ('\u{1F6C}', '\u{1F64}'),
    ('\u{1F6D}', '\u{1F65}'),
    ('\u{1F6E}', '\u{1F66}'),
    ('\u{1F6F}', '\u{1F67}'),
    ('\u{1FBA}', '\u{1F70}'),
    ('\u{1FBE}', '\u{3B9}'),
    ('\u{1FCA}', '\u{1F74}'),
    ('\u{1FFA}', '\u{1F7C}'),
    ('\u{2126}', '\u{3C9}'),
];
