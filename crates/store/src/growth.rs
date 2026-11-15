//! Growth of the arrays that follow the rows of a result. The standard
//! library doubles the room of a full array: the arrays of a large result
//! would then reserve up to twice the bytes they hold, one row of a result
//! weighing tens of bytes in all its forms, under the 4 GB of a WebAssembly
//! module (`docs/PERFORMANCES.md`, chapter 17). They grow here by a quarter
//! of what they hold: at most a quarter of their room stays empty, and each
//! value is copied four times on average as they grow.

/// Share of its length that a full array gains.
const STEP: usize = 4;

/// Room to gain for `additional` more values, `length` held.
fn room(length: usize, additional: usize) -> usize {
    additional.max(length / STEP)
}

/// Makes room in an array for `additional` more values.
pub fn reserve<T>(values: &mut Vec<T>, additional: usize) {
    if values.capacity() - values.len() < additional {
        values.reserve_exact(room(values.len(), additional));
    }
}

/// Appends a value to an array, which grows by a quarter when full.
pub fn push<T>(values: &mut Vec<T>, value: T) {
    reserve(values, 1);
    values.push(value);
}

/// Makes room in a text for `additional` more bytes.
pub fn reserve_text(text: &mut String, additional: usize) {
    if text.capacity() - text.len() < additional {
        text.reserve_exact(room(text.len(), additional));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_array_gains_a_quarter_of_its_length() {
        let mut values: Vec<u32> = Vec::new();
        for value in 0..1_000 {
            push(&mut values, value);
        }
        assert_eq!(values, (0..1_000).collect::<Vec<_>>());
        assert!(values.capacity() <= 1_000 + 1_000 / STEP);
        // A block larger than a quarter takes the room of the block alone.
        reserve(&mut values, 5_000);
        assert!(values.capacity() >= 6_000 && values.capacity() <= 6_000 + 6_000 / STEP);
        let mut text = String::from("abcd");
        reserve_text(&mut text, 10);
        assert!(text.capacity() >= 14);
    }
}
