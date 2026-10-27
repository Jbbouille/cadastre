//! "Did you mean …?" suggestions (spec § 7, Suggestions).

/// Damerau-Levenshtein distance ("optimal string alignment" variant: a transposition counts as 1).
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// Spelling variant that is always reported: case, or `_` versus `-`.
fn same_modulo_spelling(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.to_lowercase().replace('_', "-");
    a != b && norm(a) == norm(b)
}

/// The closest candidate, if close enough given the length of the written name.
pub fn closest<'c>(written: &str, candidates: impl IntoIterator<Item = &'c str>) -> Option<&'c str> {
    let max = match written.chars().count() {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    };
    candidates
        .into_iter()
        .filter(|c| *c != written)
        .filter_map(|c| {
            if same_modulo_spelling(written, c) {
                Some((0, c))
            } else {
                let d = distance(written, c);
                (max > 0 && d <= max).then_some((d, c))
            }
        })
        .min_by_key(|(d, c)| (*d, *c))
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transposition_counts_as_one() {
        assert_eq!(distance("verison", "version"), 1);
        assert_eq!(distance("os", "on"), 1);
    }

    #[test]
    fn thresholds_depend_on_length() {
        assert_eq!(closest("verison", ["version", "owner"]), Some("version"));
        assert_eq!(closest("os", ["on"]), None);
        assert_eq!(closest("stors", ["stores"]), Some("stores"));
        assert_eq!(closest("serveur", ["server", "saas"]), Some("server"));
        assert_eq!(closest("Version", ["version"]), Some("version"));
        assert_eq!(closest("on_prem", ["on-prem", "cloud"]), Some("on-prem"));
        assert_eq!(closest("cloude", ["cloud", "on-prem"]), Some("cloud"));
    }
}
