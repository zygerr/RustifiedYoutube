use rand::{Rng, seq::SliceRandom};
use rustified_core::model::Track;

pub fn shuffle_remaining<T>(queue: &mut [T], current: Option<usize>, rng: &mut impl Rng) {
    let start = current
        .map_or(0, |index| index.saturating_add(1))
        .min(queue.len());
    queue[start..].shuffle(rng);
}
pub fn restore_remaining(queue: &mut [Track], original: &[Track], current: Option<usize>) {
    let positions: std::collections::HashMap<_, _> = original
        .iter()
        .enumerate()
        .map(|(index, track)| (track.id.as_str(), index))
        .collect();
    let start = current
        .map_or(0, |index| index.saturating_add(1))
        .min(queue.len());
    queue[start..].sort_by_key(|track| {
        positions
            .get(track.id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    #[test]
    fn shuffle_preserves_the_current_song_and_previously_played_prefix() {
        let mut queue = vec![0, 1, 2, 3, 4, 5, 6, 7];
        super::shuffle_remaining(
            &mut queue,
            Some(2),
            &mut rand::rngs::StdRng::seed_from_u64(42),
        );
        assert_eq!(&queue[..3], &[0, 1, 2]);
        let mut rest = queue[3..].to_vec();
        rest.sort();
        assert_eq!(rest, vec![3, 4, 5, 6, 7]);
    }
    #[test]
    fn disabling_shuffle_restores_only_unplayed_tracks() {
        let original: Vec<_> = (0..5)
            .map(|index| rustified_core::model::Track {
                id: index.to_string(),
                ..Default::default()
            })
            .collect();
        let mut queue = vec![
            original[0].clone(),
            original[3].clone(),
            original[4].clone(),
            original[2].clone(),
            original[1].clone(),
        ];
        super::restore_remaining(&mut queue, &original, Some(1));
        assert_eq!(
            queue
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            vec!["0", "3", "1", "2", "4"]
        );
    }
}
