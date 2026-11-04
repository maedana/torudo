use std::collections::HashMap;

/// The shortest prefix of each id that no other id in `ids` shares, but never
/// shorter than `min` characters — git's abbreviated hashes, applied per item.
/// An id that runs out of characters (shorter than `min`, or a prefix of
/// another id) is given whole. Keyed by the full id.
pub fn shortest_unique_prefixes<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    min: usize,
) -> HashMap<String, String> {
    let mut sorted: Vec<Vec<char>> = ids.into_iter().map(|id| id.chars().collect()).collect();
    sorted.sort();
    sorted.dedup();

    let common = |a: &[char], b: &[char]| a.iter().zip(b).take_while(|(x, y)| x == y).count();
    sorted
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let before = i.checked_sub(1).map_or(0, |j| common(&sorted[j], id));
            let after = sorted.get(i + 1).map_or(0, |next| common(next, id));
            let len = (before.max(after) + 1).max(min).min(id.len());
            (id.iter().collect(), id[..len].iter().collect())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefixes(ids: &[&str]) -> HashMap<String, String> {
        shortest_unique_prefixes(ids.iter().copied(), 3)
    }

    #[test]
    fn distinct_ids_get_the_minimum_length() {
        let p = prefixes(&["1e156b85", "8994a0ec", "6c5d7b3a"]);
        assert_eq!(p["1e156b85"], "1e1");
        assert_eq!(p["8994a0ec"], "899");
        assert_eq!(p["6c5d7b3a"], "6c5");
    }

    #[test]
    fn only_the_colliding_ids_grow() {
        let p = prefixes(&["899a0ec", "899c123", "1e156b8"]);
        assert_eq!(p["899a0ec"], "899a");
        assert_eq!(p["899c123"], "899c");
        assert_eq!(p["1e156b8"], "1e1");
    }

    #[test]
    fn a_collision_past_the_minimum_grows_just_far_enough() {
        let p = prefixes(&["abcdef1", "abcdef2", "abcxyz9"]);
        assert_eq!(p["abcdef1"], "abcdef1");
        assert_eq!(p["abcdef2"], "abcdef2");
        assert_eq!(p["abcxyz9"], "abcx");
    }

    #[test]
    fn an_id_shorter_than_the_minimum_is_shown_whole() {
        let p = prefixes(&["t1", "abcdef"]);
        assert_eq!(p["t1"], "t1");
    }

    #[test]
    fn an_id_that_is_a_prefix_of_another_is_shown_whole() {
        let p = prefixes(&["abcd", "abcde"]);
        assert_eq!(p["abcd"], "abcd");
        assert_eq!(p["abcde"], "abcde");
    }

    #[test]
    fn a_duplicated_id_does_not_grow_against_itself() {
        let p = prefixes(&["1e156b8", "1e156b8"]);
        assert_eq!(p["1e156b8"], "1e1");
    }

    #[test]
    fn multibyte_ids_are_cut_on_char_boundaries() {
        let p = prefixes(&["あいうえお", "あいうかき"]);
        assert_eq!(p["あいうえお"], "あいうえ");
    }
}
