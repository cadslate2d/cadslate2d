//! The R2004 compression written (ODA spec §4.7): a greedy LZ77 over a
//! hash of three bytes, producing the opcodes `lz77::decompress_2004`
//! reads. Any valid stream is correct; this one favours simplicity. The
//! property test holds it to the decompressor over generated input, runs
//! longer than the window included.

/// The farthest back a match may reach: a two-byte offset past 0x3FFF
/// has fourteen bits of its own (§4.7).
const WINDOW: usize = 0x7fff;
const HASH_BITS: u32 = 15;

/// Compresses `data` as a page of a section (§4.7). The input is empty or
/// at least four bytes: a page always is, and a shorter run has no
/// encoding (`write_literals`).
pub fn compress_2004(data: &[u8]) -> Vec<u8> {
    debug_assert!(data.is_empty() || data.len() >= 4);
    let mut out = Vec::with_capacity(data.len() / 2 + 16);
    let n = data.len();
    // The first literal run is at least four bytes: a run of one to three
    // can only follow a match (§4.7).
    let mut literal_start = 0usize;
    let mut pos = 4.min(n);
    let mut table: Vec<usize> = vec![usize::MAX; 1 << HASH_BITS];
    let mut pending: Option<(usize, usize)> = None; // (distance, length)
    let hash = |at: usize| -> usize {
        let v =
            (u32::from(data[at]) << 16) | (u32::from(data[at + 1]) << 8) | u32::from(data[at + 2]);
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    };
    while pos + 3 <= n {
        let h = hash(pos);
        let candidate = table[h];
        table[h] = pos;
        let mut length = 0;
        if candidate != usize::MAX && pos - candidate <= WINDOW {
            while pos + length < n && data[candidate + length] == data[pos + length] {
                length += 1;
            }
        }
        let distance = pos - candidate.min(pos);
        // A run of three only reaches back 0x4000; farther needs four.
        if length < 3 || (length == 3 && distance > 0x4000) {
            pos += 1;
            continue;
        }
        let literals = pos - literal_start;
        let after_match = pending.is_some();
        if let Some((d, l)) = pending.take() {
            write_match(&mut out, d, l, literals);
        }
        write_literals(&mut out, &data[literal_start..pos], after_match);
        pending = Some((distance, length));
        // Index the bytes of the match too, so a later run can reach them.
        for at in pos + 1..(pos + length).min(n.saturating_sub(2)) {
            let h = hash(at);
            table[h] = at;
        }
        pos += length;
        literal_start = pos;
    }
    let literals = n - literal_start;
    let after_match = pending.is_some();
    if let Some((d, l)) = pending.take() {
        write_match(&mut out, d, l, literals);
    }
    write_literals(&mut out, &data[literal_start..], after_match);
    out.push(0x11);
    out
}

/// A literal run: its length, unless the match's opcode already said it
/// (one to three), then the bytes. The length is §4.7's literal length:
/// a byte of `n - 3` up to 0x12, else a zero and the rest in bytes of
/// 0xFF. A run of one to three bytes with no match before it cannot be
/// said, which is why `compress_2004` takes at least four bytes.
fn write_literals(out: &mut Vec<u8>, bytes: &[u8], after_match: bool) {
    let n = bytes.len();
    if n == 0 {
        return;
    }
    if !(after_match && n <= 3) {
        write_literal_length(out, n);
    }
    out.extend_from_slice(bytes);
}

fn write_literal_length(out: &mut Vec<u8>, n: usize) {
    debug_assert!(n >= 4, "a literal length is at least four");
    if n <= 0x12 {
        out.push((n - 3) as u8);
        return;
    }
    out.push(0);
    let mut rest = n - 3 - 0x0f;
    while rest > 0xff {
        out.push(0);
        rest -= 0xff;
    }
    out.push(rest as u8);
}

/// A long value (§4.7's "long compression offset"): a byte, or a zero and
/// the rest in bytes of 0xFF.
fn write_long(out: &mut Vec<u8>, value: usize) {
    debug_assert!(value >= 1);
    if value <= 0xff {
        out.push(value as u8);
        return;
    }
    out.push(0);
    let mut rest = value - 0xff;
    while rest > 0xff {
        out.push(0);
        rest -= 0xff;
    }
    out.push(rest as u8);
}

/// A match of `length` bytes `distance` back, followed by `literals`
/// bytes: the opcode says one to three of them; zero or more than three
/// are said by the literal length that follows, or by the next opcode.
fn write_match(out: &mut Vec<u8>, distance: usize, length: usize, literals: usize) {
    let inline = if (1..=3).contains(&literals) {
        literals
    } else {
        0
    };
    if length < 0x0f && distance <= 0x400 {
        let d = distance - 1;
        out.push((((length + 1) << 4) | ((d & 3) << 2) | inline) as u8);
        out.push((d >> 2) as u8);
        return;
    }
    let (base, d) = if distance <= 0x4000 {
        (0x20u8, distance - 1)
    } else {
        (0x10u8, distance - 0x4000)
    };
    if base == 0x20 {
        if (3..=33).contains(&length) {
            out.push((length + 0x1e) as u8);
        } else {
            out.push(0x20);
            write_long(out, length - 0x21);
        }
    } else if (4..=17).contains(&length) {
        out.push((0x10 + length - 2) as u8);
    } else {
        out.push(0x10);
        write_long(out, length - 9);
    }
    out.push((((d & 0x3f) << 2) | inline) as u8);
    out.push((d >> 6) as u8);
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::lz77::decompress_2004;

    fn round_trip(data: &[u8]) {
        let packed = compress_2004(data);
        let back = decompress_2004(&packed, data.len()).unwrap();
        assert_eq!(back, data, "{} bytes", data.len());
    }

    #[test]
    fn small_and_empty_inputs_round_trip() {
        round_trip(&[]);
        round_trip(&[1, 2, 3, 4]);
        round_trip(&[9, 9, 9, 9, 9]);
        round_trip(b"abcabcabcabcabcabc");
        round_trip(&[0u8; 5000]);
        round_trip(&vec![0u8; 0x7400]);
    }

    #[test]
    fn a_run_longer_than_the_window_and_input_that_does_not_shrink() {
        let long: Vec<u8> = (0..70_000u32).map(|i| (i % 7) as u8).collect();
        round_trip(&long);
        let far: Vec<u8> = (0..40_000u32)
            .map(|i| {
                if !(100..39_900).contains(&i) {
                    b"CadSlate"[(i % 8) as usize]
                } else {
                    (i * 7919 % 251) as u8
                }
            })
            .collect();
        round_trip(&far);
        let mut state = 0x2545_F491u32;
        let noise: Vec<u8> = (0..3000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 24) as u8
            })
            .collect();
        let packed = compress_2004(&noise);
        assert!(packed.len() >= noise.len());
        round_trip(&noise);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        #[test]
        fn what_is_compressed_decompresses_to_the_input(
            data in proptest::collection::vec(any::<u8>(), 4..6000),
            pattern in proptest::collection::vec(any::<u8>(), 1..40),
            repeats in 0usize..600,
        ) {
            round_trip(&data);
            let mut repeated: Vec<u8> = data.clone();
            for _ in 0..repeats {
                repeated.extend_from_slice(&pattern);
            }
            repeated.extend_from_slice(&data);
            round_trip(&repeated);
        }
    }
}
