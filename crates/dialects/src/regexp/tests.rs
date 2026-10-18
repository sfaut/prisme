//! Tests of the translation of the patterns, each form checked on the
//! servers (`crates/probe/cases/regexp-*.txt`, `regexp-classes.sql`).

use super::{Case, Library, translate};

/// Pattern of MySQL written for MariaDB, compared with the case `case`.
fn to_pcre2(pattern: &str, case: Case) -> Result<String, String> {
    translate(pattern, Library::Icu, Library::Pcre2, case).map(|translation| translation.pattern)
}

/// Pattern of MariaDB written for MySQL, compared with the case `case`.
fn to_icu(pattern: &str, case: Case) -> Result<String, String> {
    translate(pattern, Library::Pcre2, Library::Icu, case).map(|translation| translation.pattern)
}

/// Tells whether the refusal of `result` holds `words`.
fn refused(result: Result<String, String>, words: &str) -> bool {
    result.is_err_and(|reason| reason.contains(words))
}

#[test]
fn collations_give_the_case() {
    for (collation, case) in [
        ("utf8mb4_0900_ai_ci", Case::Insensitive),
        ("utf8mb4_unicode_ci", Case::Insensitive),
        ("utf8mb4_0900_as_cs", Case::Sensitive),
        ("utf8mb4_bin", Case::Sensitive),
        ("utf8mb4_nopad_bin", Case::Sensitive),
        ("latin1_general_cs", Case::Sensitive),
        ("binary", Case::Sensitive),
        ("UTF8MB4_GENERAL_CI", Case::Insensitive),
        ("utf8mb4_uca1400_ai_cs", Case::Sensitive),
        ("something", Case::Unknown),
    ] {
        assert_eq!(Case::of_collation(collation), case, "{collation}");
    }
}

#[test]
fn common_syntax_is_copied() {
    for pattern in [
        "abc",
        "^a[b-d]+c{2,3}?",
        r"(?:a|b)*+(?<n>x)\k<n>",
        r"(?i)a(?-i:b)(?s:c)",
        r"\d\D\v\V\R\p{Nd}\P{Lo}[\p{Sm}[:digit:]]",
        r"\A(?=a)(?!b)(?<=c)(?<!d)\z",
        r"a(?#comment)b",
        r"\x{41}\x41\t\n\e\cA\.\$",
        "[]a][^]a][a-][-a][a^][a&b]",
    ] {
        assert_eq!(
            to_pcre2(pattern, Case::Sensitive).as_deref(),
            Ok(pattern),
            "{pattern}"
        );
    }
    for pattern in ["abc", r"\d+[a-z]", r"x(?<n>y)\k<n>"] {
        assert_eq!(
            to_icu(pattern, Case::Sensitive).as_deref(),
            Ok(pattern),
            "{pattern}"
        );
    }
}

#[test]
fn line_terminators_follow_the_input() {
    let cases = [
        ("a.c", "(*ANY)a.c"),
        ("b$", "(*ANY)b$"),
        (r"b\Z", r"(*ANY)b\Z"),
        ("(?m)^b", "(*ANY)(?m)^b"),
        ("^b", "^b"),
        ("(?s)a.c", r"(?s)a(?>\r\n|.)c"),
        ("(?s:.)+", r"(?s:(?>\r\n|.))+"),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_pcre2(pattern, Case::Sensitive).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    let cases = [
        ("a.c", "(?d)a.c"),
        ("b$", "(?d)b$"),
        (r"b\Z", r"b(?=\n?\z)"),
        ("(?m)^b", "(?d)(?m)^b"),
        ("(?s)a.c", r"(?s)a[\s\S]c"),
        ("", "(?:)"),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_icu(pattern, Case::Sensitive).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    assert!(refused(to_pcre2("", Case::Sensitive), "empty pattern"));
    assert!(refused(
        to_pcre2("(?s)(?<=.)a", Case::Sensitive),
        "lookbehind"
    ));
}

#[test]
fn classes_follow_the_input() {
    let cases = [
        (r"\s", r"[^\S\x{180E}]"),
        (r"\S+", r"[\S\x{180E}]+"),
        (r"\h", r"[^\H\x{180E}]"),
        (r"\H", r"[\H\x{180E}]"),
        (r"[\S\n]", r"[\S\x{180E}\n]"),
        (r"[\S\x{A}é-]", r"[\S\x{180E}\né\-]"),
        (
            r"[^[:space:]a]",
            r"[^\x{9}-\x{D}\x{20}\x{85}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}a]",
        ),
        (
            r"[\h]",
            r"[\x{9}\x{20}\x{A0}\x{1680}\x{2000}-\x{200A}\x{202F}\x{205F}\x{3000}]",
        ),
        (
            r"\w",
            r"[\p{L}\p{Nl}\p{M}\p{Nd}\p{Pc}\x{200C}\x{200D}\x{24B6}-\x{24E9}\x{1F130}-\x{1F149}\x{1F150}-\x{1F169}\x{1F170}-\x{1F189}]",
        ),
        (
            r"[\W_]",
            r"(?:[^\p{L}\p{Nl}\p{M}\p{Nd}\p{Pc}\x{200C}\x{200D}\x{24B6}-\x{24E9}\x{1F130}-\x{1F149}\x{1F150}-\x{1F169}\x{1F170}-\x{1F189}]|[_])",
        ),
        (
            r"[^\W_]",
            r"(?:(?![_])[\p{L}\p{Nl}\p{M}\p{Nd}\p{Pc}\x{200C}\x{200D}\x{24B6}-\x{24E9}\x{1F130}-\x{1F149}\x{1F150}-\x{1F169}\x{1F170}-\x{1F189}])",
        ),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_pcre2(pattern, Case::Sensitive).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    let cases = [
        (r"\s", r"[\s\x{180E}]"),
        (r"\S", r"[^\s\x{180E}]"),
        (r"\h*", r"[\h\x{180E}]*"),
        (r"\H", r"[^\h\x{180E}]"),
        (r"[\sa]", r"[\s\x{180E}a]"),
        (r"[^[:blank:]a]", r"[^\h\x{180E}a]"),
        (r"[\S\n]", r"(?:[^\s\x{180E}]|[\n])"),
        (r"[^\H\t]", r"(?:(?![\t])[\h\x{180E}])"),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_icu(pattern, Case::Sensitive).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    assert!(refused(to_icu(r"\w", Case::Sensitive), "word characters"));
    assert!(refused(to_icu(r"[\W]", Case::Sensitive), "word characters"));
    assert!(refused(
        to_icu(r"[\S\H]", Case::Sensitive),
        "two complemented"
    ));
    assert!(refused(to_pcre2(r"\bword\b", Case::Sensitive), r"`\b`"));
    assert!(refused(
        to_pcre2("[[:alpha:]]", Case::Sensitive),
        "[:alpha:]"
    ));
    assert!(refused(to_pcre2(r"\X", Case::Sensitive), r"`\X`"));
}

#[test]
fn cased_categories_need_the_case() {
    assert_eq!(
        to_pcre2(r"\p{Lu}", Case::Sensitive).as_deref(),
        Ok(r"\p{Lu}")
    );
    assert_eq!(
        to_pcre2(r"(?-i)\p{L}", Case::Unknown).as_deref(),
        Ok(r"(?-i)\p{L}")
    );
    assert_eq!(
        to_pcre2(r"\p{Lo}\p{N}", Case::Insensitive).as_deref(),
        Ok(r"\p{Lo}\p{N}")
    );
    for pattern in [r"\p{Lu}", r"\P{L}", r"[\p{M}]", r"(?i)\p{Ll}"] {
        assert!(
            refused(to_pcre2(pattern, Case::Unknown), "closes the category"),
            "{pattern}"
        );
        assert!(
            refused(to_icu(pattern, Case::Insensitive), "closes the category"),
            "{pattern}"
        );
    }
    assert!(refused(
        to_pcre2(r"\p{Latin}", Case::Sensitive),
        "general categories"
    ));
    assert!(refused(to_pcre2(r"\pL", Case::Sensitive), "braces"));
}

#[test]
fn runs_of_icu_fold_fully_towards_pcre2() {
    let cases = [
        ("ß", "(?:ss|[ßẞ])"),
        ("ß+", "(?:ss|[ßẞ])+"),
        ("Strasse", "(?:St|[\u{FB05}\u{FB06}])ra(?:ss|[ßẞ])e"),
        ("Maße", "Ma(?:ss|[ßẞ])e"),
        ("sss+", "(?:ss|[ßẞ])s+"),
        ("ss+", "ss+"),
        ("ffi", "(?:ffi|f\u{FB01}|\u{FB00}i|\u{FB03})"),
        ("\u{FB05}", "(?:st|[\u{FB05}\u{FB06}])"),
        ("İ", r"(?:i\x{307}|İ)"),
        ("a(?#c)b", "a(?#c)b"),
        (r"\Qss\E", "(?:ss|[ßẞ])"),
        ("s(?i)s", "s(?i)s"),
        ("(?-i)ß", "(?-i)ß"),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_pcre2(pattern, Case::Insensitive).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    assert_eq!(to_pcre2("ß", Case::Sensitive).as_deref(), Ok("ß"));
    assert_eq!(
        to_pcre2("(?i)ss", Case::Sensitive).as_deref(),
        Ok("(?i)(?:ss|[ßẞ])")
    );
    assert!(refused(
        to_pcre2("(?<=ss)a", Case::Insensitive),
        "lookbehind"
    ));
    assert!(refused(
        to_pcre2("(ß)\\1", Case::Insensitive),
        "backreference"
    ));
    assert!(refused(
        to_pcre2(&"s".repeat(12), Case::Insensitive),
        "too many"
    ));
}

#[test]
fn runs_of_unknown_case_carry_guards() {
    let cases = [
        ("ss", "(?:ss|(?=ß)ẞ)"),
        ("ß", "(?:ß|(?=S)ss|(?=ß)ẞ)"),
        ("SS", "(?:SS|(?=S)ss|(?=ß)ẞ)"),
        ("ab", "ab"),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_pcre2(pattern, Case::Unknown).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    // A path without a character of another case, guarded by a neighbour.
    let cases = [
        ("test", "te(?:st|(?<=E)[\u{FB05}\u{FB06}])"),
        ("sta", "(?:st|[\u{FB05}\u{FB06}](?=A))a"),
        (
            "office",
            "o(?:ffi|(?=F)f\u{FB01}|\u{FB00}(?=I)i|(?<=O)\u{FB03})ce",
        ),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_pcre2(pattern, Case::Unknown).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    assert!(refused(
        to_pcre2("ffi", Case::Unknown),
        "under case insensitivity only"
    ));
    assert!(refused(
        to_pcre2("st", Case::Unknown),
        "under case insensitivity only"
    ));
    assert!(refused(
        to_pcre2("çst", Case::Unknown),
        "under case insensitivity only"
    ));
    assert!(refused(to_pcre2("(a)\\1", Case::Unknown), "backreference"));
}

#[test]
fn runs_of_pcre2_are_cut_towards_icu() {
    let cases = [
        ("ss", "s(?:)s"),
        ("Strasse", "S(?:)tras(?:)se"),
        ("ß", "[ß]"),
        ("ß+", "[ß]+"),
        ("aßb", "a[ß]b"),
        ("\u{FB05}", "(?-i:\u{FB05})"),
        ("ffi", "f(?:)f(?:)i"),
        ("abc", "abc"),
        ("i\u{307}", "i(?:)\u{307}"),
    ];
    for (pattern, written) in cases {
        assert_eq!(
            to_icu(pattern, Case::Insensitive).as_deref(),
            Ok(written),
            "{pattern}"
        );
        assert_eq!(
            to_icu(pattern, Case::Unknown).as_deref(),
            Ok(written),
            "{pattern}"
        );
    }
    assert_eq!(to_icu("ss", Case::Sensitive).as_deref(), Ok("ss"));
    assert_eq!(
        to_icu("(?i)ss", Case::Sensitive).as_deref(),
        Ok("(?i)s(?:)s")
    );
}

#[test]
fn classes_of_apart_pairs_need_the_case() {
    assert_eq!(
        to_pcre2("[\u{FB05}\u{FB06}]", Case::Insensitive).as_deref(),
        Ok("[\u{FB05}\u{FB06}]")
    );
    assert_eq!(
        to_pcre2("[\u{FB05}]", Case::Sensitive).as_deref(),
        Ok("[\u{FB05}]")
    );
    for pattern in ["[\u{FB05}]", r"[\x{1F00}-\x{1FFF}]"] {
        assert!(
            refused(to_pcre2(pattern, Case::Insensitive), "matches them"),
            "{pattern}"
        );
        assert!(
            refused(to_icu(pattern, Case::Unknown), "matches them"),
            "{pattern}"
        );
    }
}

#[test]
fn other_constructs_are_refused() {
    for pattern in [
        "a{", "a{2 }", "a{,2}", "a}", "(?x)a b", "(?w)a", "(*CR)a", "(?P<n>a)", "(?|a)", "(?(1)a)",
        r"\0", r"\12", r"\N", r"\K", r"\g1", r"\x4", "a**", "^*", "(?=a)+", "[[a]]", "[a&&b]",
        "[a--b]", "[a-\\d]", "[z-a]", "(a", "a)", r"\k{n}",
    ] {
        assert!(to_pcre2(pattern, Case::Sensitive).is_err(), "{pattern}");
    }
    assert_eq!(
        to_pcre2(r"\u00E9\U0001F600", Case::Sensitive).as_deref(),
        Ok("é\\x{1F600}")
    );
    assert!(to_icu(r"\u00E9", Case::Sensitive).is_err());
    assert!(refused(
        to_pcre2(r"(?<=a{1,2})b", Case::Sensitive),
        "variable length"
    ));
    assert!(refused(
        to_pcre2(r"(?<=(?:a|bc))d", Case::Sensitive),
        "variable length"
    ));
    assert_eq!(
        to_pcre2(r"(?<=a|bc)d", Case::Sensitive).as_deref(),
        Ok(r"(?<=a|bc)d")
    );
    assert_eq!(
        to_pcre2(r"(?<=a{2}(?:b|c))d", Case::Sensitive).as_deref(),
        Ok(r"(?<=a{2}(?:b|c))d")
    );
}
