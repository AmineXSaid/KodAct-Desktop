//! What changed between two versions of a document, as a person reads it:
//! paragraph by paragraph, and inside a changed paragraph word by word.
//! Shown by the side panel's Changes view (`ui/doc_changes.rs`).

/// One run of words in a changed paragraph.
#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    Same(String),
    Added(String),
    Removed(String),
}

/// A paragraph of the new version, against the old.
#[derive(Clone, Debug, PartialEq)]
pub enum Para {
    /// Unchanged.
    Same(String),
    /// New in this version.
    Added(String),
    /// Gone from this version.
    Removed(String),
    /// Reworded: its words, kept, added and removed.
    Changed(Vec<Run>),
}

/// The longest common subsequence's pairs (old index, new index), for
/// short lists (a document's paragraphs, a paragraph's words).
fn common<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut len = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            len[i][j] = if a[i] == b[j] {
                len[i + 1][j + 1] + 1
            } else {
                len[i + 1][j].max(len[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, vec![]);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if len[i + 1][j] >= len[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// How alike two paragraphs are, 0 to 1, by the words they share.
fn likeness(a: &str, b: &str) -> f32 {
    let (wa, wb): (Vec<&str>, Vec<&str>) = (
        a.split_whitespace().collect(),
        b.split_whitespace().collect(),
    );
    if wa.is_empty() || wb.is_empty() {
        return 0.;
    }
    if wa.len() * wb.len() > 250_000 {
        return 0.;
    }
    2. * common(&wa, &wb).len() as f32 / (wa.len() + wb.len()) as f32
}

/// A paragraph's words. A line break that starts a list item, a heading
/// or a table row is kept as a word of its own, so those stay on their
/// lines; one that only wraps prose is a space.
fn tokens(text: &str) -> Vec<&str> {
    let starts_block = |line: &str| {
        let l = line.trim_start();
        l.starts_with("- ")
            || l.starts_with("* ")
            || l.starts_with('#')
            || l.starts_with('|')
            || l.starts_with('>')
            || l.split_once(". ")
                .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    let mut out = vec![];
    for (i, line) in text.lines().enumerate() {
        if i > 0 && starts_block(line) {
            out.push("\n");
        }
        out.extend(line.split_whitespace());
    }
    out
}

/// Words joined back: a space between them, none around a line break.
pub fn join(left: &mut String, word: &str) {
    if !left.is_empty() && !left.ends_with('\n') && word != "\n" {
        left.push(' ');
    }
    left.push_str(word);
}

/// A paragraph reworded, word by word.
fn words(old: &str, new: &str) -> Vec<Run> {
    let (a, b) = (tokens(old), tokens(new));
    let pairs = common(&a, &b);
    let mut out: Vec<Run> = vec![];
    let push = |out: &mut Vec<Run>, run: Run| match (out.last_mut(), &run) {
        (Some(Run::Same(s)), Run::Same(t))
        | (Some(Run::Added(s)), Run::Added(t))
        | (Some(Run::Removed(s)), Run::Removed(t)) => join(s, t),
        _ => out.push(run),
    };
    let (mut i, mut j) = (0, 0);
    for (pi, pj) in pairs.into_iter().chain([(a.len(), b.len())]) {
        for w in &a[i..pi] {
            push(&mut out, Run::Removed(w.to_string()));
        }
        for w in &b[j..pj] {
            push(&mut out, Run::Added(w.to_string()));
        }
        if pi < a.len() {
            push(&mut out, Run::Same(a[pi].to_string()));
        }
        (i, j) = (pi + 1, pj + 1);
    }
    out
}

/// The paragraphs of `text`: blocks split by blank lines.
fn paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .take(1500)
        .collect()
}

/// `new` against `old`, paragraph by paragraph.
pub fn diff(old: &str, new: &str) -> Vec<Para> {
    let (a, b) = (paragraphs(old), paragraphs(new));
    let pairs = common(&a, &b);
    let mut out = vec![];
    let (mut i, mut j) = (0, 0);
    for (pi, pj) in pairs.into_iter().chain([(a.len(), b.len())]) {
        // Between two kept paragraphs: pair the reworded ones, the rest
        // were removed or added.
        let (gone, new_here) = (&a[i..pi], &b[j..pj]);
        let mut used = vec![false; gone.len()];
        let mut removed_first = vec![];
        for n in new_here {
            let best = gone
                .iter()
                .enumerate()
                .filter(|(k, _)| !used[*k])
                .map(|(k, g)| (k, likeness(g, n)))
                .max_by(|x, y| x.1.total_cmp(&y.1));
            match best {
                Some((k, score)) if score >= 0.4 => {
                    // Paragraphs removed before this one show first.
                    for (r, g) in gone.iter().enumerate().take(k) {
                        if !used[r] {
                            used[r] = true;
                            removed_first.push(Para::Removed(g.clone()));
                        }
                    }
                    out.append(&mut removed_first);
                    used[k] = true;
                    out.push(Para::Changed(words(&gone[k], n)));
                }
                _ => out.push(Para::Added(n.clone())),
            }
        }
        for (k, g) in gone.iter().enumerate() {
            if !used[k] {
                out.push(Para::Removed(g.clone()));
            }
        }
        if pi < a.len() {
            out.push(Para::Same(a[pi].clone()));
        }
        (i, j) = (pi + 1, pj + 1);
    }
    out
}

/// How many paragraphs changed, were added and were removed.
pub fn counts(paras: &[Para]) -> (usize, usize, usize) {
    paras.iter().fold((0, 0, 0), |(c, a, r), p| match p {
        Para::Changed(_) => (c + 1, a, r),
        Para::Added(_) => (c, a + 1, r),
        Para::Removed(_) => (c, a, r + 1),
        Para::Same(_) => (c, a, r),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reworded_paragraph_shows_its_words() {
        let old = "# Budget\n\nLicences fall to 640 EUR.\n\nTravel is flat.";
        let new = "# Budget\n\nLicences fall to 700 EUR.\n\nTravel is flat.\n\nNext: sign off.";
        let d = diff(old, new);
        assert_eq!(d[0], Para::Same("# Budget".into()));
        assert_eq!(
            d[1],
            Para::Changed(vec![
                Run::Same("Licences fall to".into()),
                Run::Removed("640".into()),
                Run::Added("700".into()),
                Run::Same("EUR.".into()),
            ])
        );
        assert_eq!(d[2], Para::Same("Travel is flat.".into()));
        assert_eq!(d[3], Para::Added("Next: sign off.".into()));
        assert_eq!(counts(&d), (1, 1, 0));
    }

    #[test]
    fn a_dropped_paragraph_is_removed_not_reworded() {
        let d = diff(
            "A one.\n\nSomething else entirely here.\n\nC three.",
            "A one.\n\nC three.",
        );
        assert_eq!(
            d,
            vec![
                Para::Same("A one.".into()),
                Para::Removed("Something else entirely here.".into()),
                Para::Same("C three.".into()),
            ]
        );
    }

    #[test]
    fn list_items_keep_their_lines_and_prose_rejoins() {
        let d = diff(
            "Wrapped\nprose here.\n\n1. Read it.\n2. Keep it for a release.",
            "Wrapped\nprose there.\n\n1. Read it.\n2. Keep it for two releases.",
        );
        let Para::Changed(runs) = &d[0] else {
            panic!("{d:?}")
        };
        assert_eq!(runs[0], Run::Same("Wrapped prose".into()));
        let Para::Changed(runs) = &d[1] else {
            panic!("{d:?}")
        };
        assert_eq!(runs[0], Run::Same("1. Read it.\n2. Keep it for".into()));
    }

    #[test]
    fn the_same_text_has_no_changes() {
        assert_eq!(counts(&diff("a\n\nb", "a\n\nb")), (0, 0, 0));
    }
}
