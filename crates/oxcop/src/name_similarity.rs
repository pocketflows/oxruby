//! Port of `lib/rubocop/name_similarity.rb`, with the parts of Ruby's
//! `did_you_mean` it relies on (`DidYouMean::SpellChecker`, `Jaro`,
//! `JaroWinkler` and `Levenshtein`, as shipped with Ruby 3.3).

/// `find_similar_name(target_name, names)`.
pub fn find_similar_name(target_name: &str, names: &[&str]) -> Option<String> {
    find_similar_names(target_name, names).into_iter().next()
}

/// `find_similar_names(target_name, names)`.
pub fn find_similar_names(target_name: &str, names: &[&str]) -> Vec<String> {
    let names: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| *name != target_name)
        .collect();
    spell_checker_correct(&names, target_name)
}

fn normalize(s: &str) -> String {
    s.to_lowercase().replace('@', "")
}

/// `DidYouMean::SpellChecker.new(dictionary:).correct(input)`.
pub fn spell_checker_correct(dictionary: &[&str], input: &str) -> Vec<String> {
    let normalized_input = normalize(input);
    let input_length = normalized_input.chars().count();
    let threshold = if input_length > 3 { 0.834 } else { 0.77 };

    let mut words: Vec<&str> = dictionary
        .iter()
        .copied()
        .filter(|word| jaro_winkler_distance(&normalize(word), &normalized_input) >= threshold)
        .collect();
    words.retain(|word| input != *word);
    words.sort_by(|a, b| {
        jaro_winkler_distance(a, &normalized_input)
            .partial_cmp(&jaro_winkler_distance(b, &normalized_input))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    words.reverse();

    // Correct mistypes
    let threshold = (input_length as f64 * 0.25).ceil() as usize;
    let mut corrections: Vec<String> = words
        .iter()
        .filter(|c| levenshtein_distance(&normalize(c), &normalized_input) <= threshold)
        .map(|s| (*s).to_owned())
        .collect();

    // Correct misspells
    if corrections.is_empty() {
        corrections = words
            .iter()
            .filter(|word| {
                let word = normalize(word);
                let word_length = word.chars().count();
                let length = if input_length < word_length {
                    input_length
                } else {
                    word_length
                };
                levenshtein_distance(&word, &normalized_input) < length
            })
            .take(1)
            .map(|s| (*s).to_owned())
            .collect();
    }

    corrections
}

/// `DidYouMean::Jaro.distance`.
pub fn jaro_distance(str1: &str, str2: &str) -> f64 {
    let mut s1: Vec<char> = str1.chars().collect();
    let mut s2: Vec<char> = str2.chars().collect();
    if s1.len() > s2.len() {
        std::mem::swap(&mut s1, &mut s2);
    }
    let (length1, length2) = (s1.len(), s2.len());

    let mut m = 0.0_f64;
    let mut t = 0.0_f64;
    let range = (length2 / 2).saturating_sub(1);
    let mut flags1 = vec![false; length1 + 1];
    let mut flags2 = vec![false; length2 + range + length1 + 2];

    let mut i = 0;
    while i < length1 {
        let last = i + range;
        let mut j = i.saturating_sub(range);
        while j <= last {
            if !flags2[j] && s2.get(j) == Some(&s1[i]) {
                flags2[j] = true;
                flags1[i] = true;
                m += 1.0;
                break;
            }
            j += 1;
        }
        i += 1;
    }

    let mut k = 0;
    i = 0;
    while i < length1 {
        if flags1[i] {
            let mut j = k;
            let mut index = k;
            // `k = while j < length2 ... break(j + 1) ... end` (nil when the
            // loop finishes, which Ruby then uses as 0 via `j = index = k`
            // raising; it can't finish because a flag is always found).
            let mut next_k = None;
            while j < length2 {
                index = j;
                if flags2[j] {
                    next_k = Some(j + 1);
                    break;
                }
                j += 1;
            }
            k = next_k.unwrap_or(0);
            if s2.get(index) != Some(&s1[i]) {
                t += 1.0;
            }
        }
        i += 1;
    }
    t = (t / 2.0).floor();

    if m == 0.0 {
        0.0
    } else {
        (m / length1 as f64 + m / length2 as f64 + (m - t) / m) / 3.0
    }
}

/// `DidYouMean::JaroWinkler.distance`.
pub fn jaro_winkler_distance(str1: &str, str2: &str) -> f64 {
    const WEIGHT: f64 = 0.1;
    const THRESHOLD: f64 = 0.7;
    let jaro_distance = jaro_distance(str1, str2);

    if jaro_distance > THRESHOLD {
        let codepoints2: Vec<char> = str2.chars().collect();
        let mut prefix_bonus = 0;
        for (i, char1) in str1.chars().enumerate() {
            if codepoints2.get(i) == Some(&char1) && i < 4 {
                prefix_bonus += 1;
            } else {
                break;
            }
        }
        jaro_distance + (prefix_bonus as f64 * WEIGHT * (1.0 - jaro_distance))
    } else {
        jaro_distance
    }
}

/// `DidYouMean::Levenshtein.distance`.
pub fn levenshtein_distance(str1: &str, str2: &str) -> usize {
    let s1: Vec<char> = str1.chars().collect();
    let s2: Vec<char> = str2.chars().collect();
    let n = s1.len();
    let m = s2.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut d: Vec<usize> = (0..=m).collect();
    let mut x = 0;

    for (i, &char1) in s1.iter().enumerate() {
        let mut i = i + 1;
        let mut j = 0;
        while j < m {
            let cost = usize::from(char1 != s2[j]);
            x = min3(d[j + 1] + 1, i + 1, d[j] + cost);
            d[j] = i;
            i = x;
            j += 1;
        }
        d[m] = x;
    }

    x
}

fn min3(a: usize, b: usize, c: usize) -> usize {
    if a < b && a < c {
        a
    } else if b < c {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_names() {
        assert_eq!(
            find_similar_name("foo", &["fooo", "bar"]),
            Some("fooo".to_owned())
        );
        assert_eq!(find_similar_name("foo", &["bar", "baz"]), None);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }
}
