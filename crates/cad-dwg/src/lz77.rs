//! The R2004 compression (ODA spec §4.7) and the R2007 one (§5.10): two
//! LZ77 variants, decompressed into a buffer that grows with the bytes
//! produced and never past the size the caller allows.

use crate::DwgError;

struct Input<'a> {
    src: &'a [u8],
    at: usize,
}

impl Input<'_> {
    fn byte(&mut self) -> Result<u8, DwgError> {
        let b = *self.src.get(self.at).ok_or(DwgError::Truncated {
            what: "compressed data",
            at: self.at,
        })?;
        self.at += 1;
        Ok(b)
    }

    fn corrupt(&self, what: &'static str) -> DwgError {
        DwgError::Corrupt { what, at: self.at }
    }
}

/// Copies `len` bytes from `offset` back in `out` to its end: the run may
/// overlap itself, as LZ77 runs do.
fn copy_back(out: &mut Vec<u8>, offset: usize, len: usize, limit: usize) -> Result<(), DwgError> {
    if offset == 0 || offset > out.len() {
        return Err(DwgError::Corrupt {
            what: "a compression offset",
            at: out.len(),
        });
    }
    if out.len() + len > limit {
        return Err(DwgError::Corrupt {
            what: "a compressed run past the section",
            at: out.len(),
        });
    }
    let start = out.len() - offset;
    for k in 0..len {
        let b = out[start + k];
        out.push(b);
    }
    Ok(())
}

/// Decompresses R2004 section data (§4.7) to at most `out_size` bytes:
/// what the input yields, which a writer pads to the page size (§4.5) and
/// the caller cuts to the logical size.
pub fn decompress_2004(src: &[u8], out_size: usize) -> Result<Vec<u8>, DwgError> {
    let mut input = Input { src, at: 0 };
    let mut out: Vec<u8> = Vec::new();
    // Literal length: `first` is the byte already read.
    fn literal_length(input: &mut Input, first: u8) -> Result<usize, DwgError> {
        if first == 0 {
            let mut total = 0x0f;
            loop {
                let b = input.byte()?;
                if b == 0 {
                    total += 0xff;
                } else {
                    total += usize::from(b);
                    break;
                }
            }
            Ok(total + 3)
        } else {
            Ok(usize::from(first & 0x0f) + 3)
        }
    }
    fn long_offset(input: &mut Input) -> Result<usize, DwgError> {
        let b = input.byte()?;
        if b != 0 {
            return Ok(usize::from(b));
        }
        let mut total = 0xff;
        loop {
            let b = input.byte()?;
            if b == 0 {
                total += 0xff;
            } else {
                total += usize::from(b);
                break;
            }
        }
        Ok(total)
    }
    fn two_byte_offset(input: &mut Input) -> Result<(usize, usize), DwgError> {
        let b1 = usize::from(input.byte()?);
        let b2 = usize::from(input.byte()?);
        Ok(((b1 >> 2) | (b2 << 6), b1 & 3))
    }

    let first = input.byte()?;
    let mut literal = if first & 0xf0 == 0 {
        literal_length(&mut input, first)?
    } else {
        0
    };
    let mut opcode = if literal == 0 { first } else { 0 };
    loop {
        if out.len() + literal > out_size {
            return Err(input.corrupt("a literal run past the section"));
        }
        for _ in 0..literal {
            out.push(input.byte()?);
        }
        if literal != 0 {
            if out.len() >= out_size && input.at >= src.len() {
                break;
            }
            opcode = input.byte()?;
        }
        let (run, offset, lit_count) = match opcode {
            0x00..=0x0f => return Err(input.corrupt("a compression opcode")),
            0x10 => {
                let run = long_offset(&mut input)? + 9;
                let (offset, lit) = two_byte_offset(&mut input)?;
                (run, offset + 0x3fff, lit)
            }
            0x11 => break,
            0x12..=0x1f => {
                let run = usize::from(opcode & 0x0f) + 2;
                let (offset, lit) = two_byte_offset(&mut input)?;
                (run, offset + 0x3fff, lit)
            }
            0x20 => {
                let run = long_offset(&mut input)? + 0x21;
                let (offset, lit) = two_byte_offset(&mut input)?;
                (run, offset, lit)
            }
            0x21..=0x3f => {
                let run = usize::from(opcode) - 0x1e;
                let (offset, lit) = two_byte_offset(&mut input)?;
                (run, offset, lit)
            }
            _ => {
                let run = (usize::from(opcode & 0xf0) >> 4) - 1;
                let b2 = usize::from(input.byte()?);
                let offset = (b2 << 2) | (usize::from(opcode & 0x0c) >> 2);
                (run, offset, usize::from(opcode & 3))
            }
        };
        copy_back(&mut out, offset + 1, run, out_size)?;
        literal = if lit_count == 0 {
            let b = input.byte()?;
            if b & 0xf0 != 0 {
                opcode = b;
                0
            } else {
                literal_length(&mut input, b)?
            }
        } else {
            lit_count
        };
        if literal == 0 && out.len() >= out_size && input.at >= src.len() {
            break;
        }
    }
    Ok(out)
}

/// The R2007 literal copy permutes the bytes of each run (§5.10.1).
fn copy_literal_2007(out: &mut Vec<u8>, src: &[u8]) {
    fn step(out: &mut Vec<u8>, src: &[u8], n: usize, o: usize) {
        match n {
            1 => out.push(src[o]),
            2 => out.extend_from_slice(&[src[o + 1], src[o]]),
            3 => out.extend_from_slice(&[src[o + 2], src[o + 1], src[o]]),
            4 | 8 => out.extend_from_slice(&src[o..o + n]),
            _ => {
                out.extend_from_slice(&src[o + 8..o + 16]);
                out.extend_from_slice(&src[o..o + 8]);
            }
        }
    }
    let mut src = src;
    while src.len() >= 32 {
        step(out, src, 16, 16);
        step(out, src, 16, 0);
        src = &src[32..];
    }
    const TABLE: [&[(usize, usize)]; 32] = [
        &[],
        &[(1, 0)],
        &[(2, 0)],
        &[(3, 0)],
        &[(4, 0)],
        &[(1, 4), (4, 0)],
        &[(1, 5), (4, 1), (1, 0)],
        &[(2, 5), (4, 1), (1, 0)],
        &[(8, 0)],
        &[(1, 8), (8, 0)],
        &[(1, 9), (8, 1), (1, 0)],
        &[(2, 9), (8, 1), (1, 0)],
        &[(4, 8), (8, 0)],
        &[(1, 12), (4, 8), (8, 0)],
        &[(1, 13), (4, 9), (8, 1), (1, 0)],
        &[(2, 13), (4, 9), (8, 1), (1, 0)],
        &[(16, 0)],
        &[(8, 9), (1, 8), (8, 0)],
        &[(1, 17), (16, 1), (1, 0)],
        &[(3, 16), (16, 0)],
        &[(4, 16), (16, 0)],
        &[(1, 20), (4, 16), (16, 0)],
        &[(2, 20), (4, 16), (16, 0)],
        &[(3, 20), (4, 16), (16, 0)],
        &[(8, 16), (16, 0)],
        &[(8, 17), (1, 16), (16, 0)],
        &[(1, 25), (8, 17), (1, 16), (16, 0)],
        &[(2, 25), (8, 17), (1, 16), (16, 0)],
        &[(4, 24), (8, 16), (16, 0)],
        &[(1, 28), (4, 24), (8, 16), (16, 0)],
        &[(2, 28), (4, 24), (8, 16), (16, 0)],
        &[(1, 30), (4, 26), (8, 18), (16, 2), (2, 0)],
    ];
    for &(n, o) in TABLE[src.len()] {
        step(out, src, n, o);
    }
}

/// Decompresses R2007 page data (§5.10) to exactly `out_size` bytes.
pub fn decompress_2007(src: &[u8], out_size: usize) -> Result<Vec<u8>, DwgError> {
    let mut input = Input { src, at: 0 };
    let mut out: Vec<u8> = Vec::new();
    fn literal_length(input: &mut Input, opcode: u8) -> Result<usize, DwgError> {
        let mut length = usize::from(opcode) + 8;
        if length == 0x17 {
            let n = usize::from(input.byte()?);
            length += n;
            if n == 0xff {
                loop {
                    let lo = usize::from(input.byte()?);
                    let hi = usize::from(input.byte()?);
                    let n = lo | (hi << 8);
                    length += n;
                    if n != 0xffff {
                        break;
                    }
                }
            }
        }
        Ok(length)
    }
    /// The offset and length of a back copy; the opcode is advanced.
    fn instructions(input: &mut Input, opcode: &mut u8) -> Result<(usize, usize), DwgError> {
        Ok(match *opcode >> 4 {
            0 => {
                let mut length = usize::from(*opcode & 0xf) + 0x13;
                let mut offset = usize::from(input.byte()?);
                *opcode = input.byte()?;
                length += (usize::from(*opcode) >> 3) & 0x10;
                offset += ((usize::from(*opcode) & 0x78) << 5) + 1;
                (offset, length)
            }
            1 => {
                let length = usize::from(*opcode & 0xf) + 3;
                let mut offset = usize::from(input.byte()?);
                *opcode = input.byte()?;
                offset += ((usize::from(*opcode) & 0xf8) << 5) + 1;
                (offset, length)
            }
            2 => {
                let mut offset = usize::from(input.byte()?);
                offset |= usize::from(input.byte()?) << 8;
                let mut length = usize::from(*opcode & 7);
                if *opcode & 8 == 0 {
                    *opcode = input.byte()?;
                    length += usize::from(*opcode & 0xf8);
                } else {
                    offset += 1;
                    length += usize::from(input.byte()?) << 3;
                    *opcode = input.byte()?;
                    length += ((usize::from(*opcode) & 0xf8) << 8) + 0x100;
                }
                (offset, length)
            }
            _ => {
                let length = usize::from(*opcode) >> 4;
                let mut offset = usize::from(*opcode & 15);
                *opcode = input.byte()?;
                offset += ((usize::from(*opcode) & 0xf8) << 1) + 1;
                (offset, length)
            }
        })
    }

    let mut opcode = input.byte()?;
    let mut length = 0usize;
    if opcode & 0xf0 == 0x20 {
        input.byte()?;
        input.byte()?;
        length = usize::from(input.byte()? & 7);
        if length == 0 {
            return Err(input.corrupt("a compression header"));
        }
    }
    while input.at < src.len() {
        if length == 0 {
            length = literal_length(&mut input, opcode)?;
        }
        if out.len() + length > out_size || input.at + length > src.len() {
            return Err(input.corrupt("a literal run past the section"));
        }
        copy_literal_2007(&mut out, &src[input.at..input.at + length]);
        input.at += length;
        if input.at >= src.len() {
            break;
        }
        opcode = input.byte()?;
        let (mut offset, mut run) = instructions(&mut input, &mut opcode)?;
        loop {
            copy_back(&mut out, offset, run, out_size)?;
            length = usize::from(opcode & 7);
            if length != 0 || input.at >= src.len() {
                break;
            }
            opcode = input.byte()?;
            if opcode >> 4 == 0 {
                break;
            }
            if opcode >> 4 == 0xf {
                opcode &= 0xf;
            }
            (offset, run) = instructions(&mut input, &mut opcode)?;
        }
    }
    if out.len() != out_size {
        return Err(input.corrupt("a page shorter than its size"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_specifications_literal_lengths_and_long_offsets() {
        // §4.7: a literal run of 8, then a run of 0x14 from 0x00 0x02.
        let mut src = vec![0x05];
        src.extend([b'a'; 8]);
        src.push(0x11);
        assert_eq!(decompress_2004(&src, 8).unwrap(), [b'a'; 8]);
        // Room to spare is not an error: the caller cuts to what it needs.
        assert_eq!(decompress_2004(&src, 100).unwrap(), [b'a'; 8]);
        let mut src = vec![0x00, 0x02];
        src.extend([b'b'; 0x14]);
        src.push(0x11);
        assert_eq!(decompress_2004(&src, 0x14).unwrap(), [b'b'; 0x14]);
        // Opcode 0x20 with a long offset 0xDD: a run of 0xDD + 0x21 bytes
        // copied from one byte back, after a literal of four.
        let mut src = vec![0x01]; // literal length (1 & 0x0f) + 3
        src.extend([1, 2, 3, 4]);
        src.push(0x20);
        src.push(0xDD);
        src.extend([0x00, 0x00]); // two-byte offset 0 → copy from 1 back, lit count 0
        src.push(0x11);
        let want = 4 + 0xDD + 0x21;
        let out = decompress_2004(&src, want).unwrap();
        assert_eq!(out.len(), want);
        assert!(out[4..].iter().all(|&b| b == 4));
        // The long form 0x00 0x00 0x34 → 0x232.
        let mut src = vec![0x01];
        src.extend([7, 7, 7, 7]);
        src.push(0x20);
        src.extend([0x00, 0x00, 0x34]);
        src.extend([0x00, 0x00]);
        src.push(0x11);
        let want = 4 + 0x232 + 0x21;
        assert_eq!(decompress_2004(&src, want).unwrap().len(), want);
    }

    #[test]
    fn a_short_opcode_copies_a_run_and_bad_input_is_refused() {
        // 0x40..0xff: run ((op & 0xf0) >> 4) - 1, offset from the next byte.
        let mut src = vec![0x01];
        src.extend([1, 2, 3, 4]);
        src.push(0x43); // run 3, offset bits 0, lit count 3
        src.push(0x00); // offset 0 → 1 back
        src.extend([9, 9, 9]);
        src.push(0x11);
        assert_eq!(
            decompress_2004(&src, 10).unwrap(),
            [1, 2, 3, 4, 4, 4, 4, 9, 9, 9]
        );
        // A back copy before the start, an opcode in the unused range, and
        // a literal past the size.
        assert!(matches!(
            decompress_2004(&[0x43, 0x10, 0x11], 4),
            Err(DwgError::Corrupt { .. })
        ));
        let mut src = vec![0x01];
        src.extend([1, 2, 3, 4]);
        src.push(0x0a);
        assert!(matches!(
            decompress_2004(&src, 10),
            Err(DwgError::Corrupt { .. })
        ));
        assert!(matches!(
            decompress_2004(&[0x05, 1, 2], 8),
            Err(DwgError::Truncated { .. })
        ));
        assert!(matches!(
            decompress_2004(&[0x05, 1, 2, 3, 4, 5, 6, 7, 8, 0x11], 4),
            Err(DwgError::Corrupt { .. })
        ));
    }

    #[test]
    fn the_2007_literal_copy_permutes_each_run() {
        let mut out = Vec::new();
        copy_literal_2007(&mut out, &[1, 2, 3]);
        assert_eq!(out, [3, 2, 1]);
        out.clear();
        copy_literal_2007(&mut out, &[1, 2, 3, 4, 5]);
        assert_eq!(out, [5, 1, 2, 3, 4]);
        out.clear();
        let run: Vec<u8> = (0..32).collect();
        copy_literal_2007(&mut out, &run);
        let mut want: Vec<u8> = (24..32).collect();
        want.extend(16..24);
        want.extend(8..16);
        want.extend(0..8);
        assert_eq!(out, want);
        // Every length from 0 to 31 is a permutation of its input.
        for n in 0..32u8 {
            let run: Vec<u8> = (0..n).collect();
            out.clear();
            copy_literal_2007(&mut out, &run);
            let mut sorted = out.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, run, "length {n}");
        }
    }

    #[test]
    fn a_2007_stream_of_one_literal_and_one_back_copy() {
        // Opcode 0x03 → literal length 11: the bytes permuted; then opcode
        // 0x40: run 4, offset from the next byte (0 → 1 back), lit 0; then
        // the end of input.
        let mut src = vec![0x03];
        src.extend([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        src.extend([0x40, 0x00]);
        let out = decompress_2007(&src, 15).unwrap();
        assert_eq!(out[..11], [11, 10, 2, 3, 4, 5, 6, 7, 8, 9, 1][..]);
        assert_eq!(out[11..], [1, 1, 1, 1]);
        assert!(matches!(
            decompress_2007(&src, 20),
            Err(DwgError::Corrupt { .. })
        ));
        assert!(matches!(
            decompress_2007(&[0x03, 1], 11),
            Err(DwgError::Corrupt { .. })
        ));
    }
}
