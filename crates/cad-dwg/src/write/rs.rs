//! Reed–Solomon encoding (ODA spec §5.13): (255, 239) for the file header
//! and the system pages, (255, 251) for data pages; codewords interleaved
//! byte by byte, or laid out data first with the parity after (a page of
//! encoding 1). The test decoder below is independent of the encoder and
//! corrects up to two bytes of a data block, as the specification promises.

/// The primitive polynomial of the data pages' field (§5.13).
pub const DATA_POLY: u16 = 0x11D;
/// The primitive polynomial of the system pages' field.
pub const SYSTEM_POLY: u16 = 0x169;
pub const DATA_K: usize = 251;
pub const SYSTEM_K: usize = 239;
const N: usize = 255;

/// GF(2^8) under one primitive polynomial: exp and log tables.
pub struct Field {
    exp: [u8; 512],
    log: [u8; 256],
}

impl Field {
    pub fn new(poly: u16) -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x: u16 = 1;
        for (i, entry) in exp.iter_mut().enumerate().take(255) {
            *entry = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= poly;
            }
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        Self { exp, log }
    }

    pub fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[usize::from(self.log[usize::from(a)]) + usize::from(self.log[usize::from(b)])]
        }
    }

    #[cfg(test)]
    pub fn inverse(&self, a: u8) -> u8 {
        self.exp[255 - usize::from(self.log[usize::from(a)])]
    }

    pub fn alpha(&self, power: usize) -> u8 {
        self.exp[power % 255]
    }

    /// The generator polynomial of `nsym` parity symbols with the first
    /// consecutive root at α^fcr, lowest degree first; monic.
    pub fn generator(&self, nsym: usize, fcr: usize) -> Vec<u8> {
        let mut g = vec![1u8];
        for i in 0..nsym {
            let root = self.alpha(fcr + i);
            let mut next = vec![0u8; g.len() + 1];
            for (j, &c) in g.iter().enumerate() {
                next[j + 1] ^= c;
                next[j] ^= self.mul(c, root);
            }
            g = next;
        }
        g
    }
}

/// One codeword: `k` data bytes (fewer are padded with zeros) followed by
/// `255 - k` parity bytes, the first consecutive root at α^k (§5.13).
pub fn codeword(field: &Field, data: &[u8], k: usize) -> [u8; N] {
    let nsym = N - k;
    let g = field.generator(nsym, k % N);
    debug_assert_eq!(g[nsym], 1, "the generator is monic");
    let taken = data.len().min(k);
    let mut out = [0u8; N];
    out[..taken].copy_from_slice(&data[..taken]);
    // The division of data·x^nsym by g as a shift register: the register
    // holds the remainder, highest degree at its end.
    let mut register = vec![0u8; nsym];
    for &byte in &out[..k] {
        let feedback = byte ^ register[nsym - 1];
        for j in (1..nsym).rev() {
            register[j] = register[j - 1] ^ field.mul(feedback, g[j]);
        }
        register[0] = field.mul(feedback, g[0]);
    }
    for j in 0..nsym {
        out[k + j] = register[nsym - 1 - j];
    }
    out
}

/// `blocks` codewords of `k` data bytes each, interleaved: byte `i` of
/// block `b` lands at `i * blocks + b` (§5.13). The data is padded with
/// zeros to `blocks * k`.
pub fn interleaved(field: &Field, data: &[u8], blocks: usize, k: usize) -> Vec<u8> {
    let mut out = vec![0u8; blocks * N];
    for b in 0..blocks {
        let start = (b * k).min(data.len());
        let end = ((b + 1) * k).min(data.len());
        let word = codeword(field, &data[start..end], k);
        for (i, &byte) in word.iter().enumerate() {
            out[i * blocks + b] = byte;
        }
    }
    out
}

/// A data page of encoding 1: the data padded to a multiple of eight, then
/// each block's parity, interleaved among the blocks.
pub fn data_then_parity(field: &Field, data: &[u8], k: usize) -> Vec<u8> {
    let aligned = data.len().div_ceil(8) * 8;
    let blocks = aligned.div_ceil(k).max(1);
    let mut padded = data.to_vec();
    padded.resize(aligned, 0);
    let mut out = padded.clone();
    let mut parities: Vec<[u8; N]> = Vec::with_capacity(blocks);
    for b in 0..blocks {
        let start = (b * k).min(padded.len());
        let end = ((b + 1) * k).min(padded.len());
        parities.push(codeword(field, &padded[start..end], k));
    }
    for word in &parities {
        out.extend_from_slice(&word[k..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decoder of the test's own: syndromes, a Peterson–Gorenstein–Zierler
    /// locator for one or two errors, a Chien search, and Forney's magnitudes.
    fn decode(field: &Field, word: &[u8; N], k: usize) -> Result<[u8; N], &'static str> {
        let nsym = N - k;
        let fcr = k % N;
        let eval =
            |poly: &[u8], x: u8| poly.iter().rev().fold(0u8, |acc, &c| field.mul(acc, x) ^ c);
        // word[0] is the highest power: evaluate as a polynomial in x.
        let as_poly: Vec<u8> = word.iter().rev().copied().collect();
        let syndromes: Vec<u8> = (0..nsym)
            .map(|j| eval(&as_poly, field.alpha(fcr + j)))
            .collect();
        if syndromes.iter().all(|&s| s == 0) {
            return Ok(*word);
        }
        // Two errors: s0 s1; s1 s2 | s2 s3  → λ1, λ2 of Λ(x) = 1 + λ1 x + λ2 x².
        let (s0, s1, s2, s3) = (syndromes[0], syndromes[1], syndromes[2], syndromes[3]);
        let det = field.mul(s0, s2) ^ field.mul(s1, s1);
        let (lambda1, lambda2) = if det != 0 {
            let inv = field.inverse(det);
            let l1 = field.mul(field.mul(s1, s2) ^ field.mul(s0, s3), inv);
            let l2 = field.mul(field.mul(s1, s3) ^ field.mul(s2, s2), inv);
            (l1, l2)
        } else {
            // One error: Λ(x) = 1 + (s1/s0) x.
            (field.mul(s1, field.inverse(s0)), 0)
        };
        // Chien search: positions i where Λ(α^-i) = 0.
        let mut positions = Vec::new();
        for i in 0..N {
            let x = field.alpha(255 - i % 255);
            let value = 1 ^ field.mul(lambda1, x) ^ field.mul(lambda2, field.mul(x, x));
            if value == 0 {
                positions.push(i);
            }
        }
        if positions.is_empty() || positions.len() > 2 {
            return Err("the errors are more than two");
        }
        // Magnitudes from the syndrome equations: s_j = Σ e_p · (α^p)^(fcr + j).
        let mut fixed = *word;
        let x: Vec<u8> = positions.iter().map(|&p| field.alpha(p)).collect();
        let powered = |p: u8, e: usize| (0..e).fold(1u8, |acc, _| field.mul(acc, p));
        if positions.len() == 1 {
            let magnitude = field.mul(s0, field.inverse(powered(x[0], fcr)));
            fixed[N - 1 - positions[0]] ^= magnitude;
        } else {
            let a = powered(x[0], fcr);
            let b = powered(x[1], fcr);
            let c = powered(x[0], fcr + 1);
            let d = powered(x[1], fcr + 1);
            let det = field.mul(a, d) ^ field.mul(b, c);
            let inv = field.inverse(det);
            let e0 = field.mul(field.mul(s0, d) ^ field.mul(b, s1), inv);
            let e1 = field.mul(field.mul(a, s1) ^ field.mul(c, s0), inv);
            fixed[N - 1 - positions[0]] ^= e0;
            fixed[N - 1 - positions[1]] ^= e1;
        }
        let as_poly: Vec<u8> = fixed.iter().rev().copied().collect();
        if (0..nsym).any(|j| eval(&as_poly, field.alpha(fcr + j)) != 0) {
            return Err("the correction leaves a syndrome");
        }
        Ok(fixed)
    }

    #[test]
    fn the_fields_are_the_specifications() {
        let data = Field::new(DATA_POLY);
        assert_eq!(data.alpha(0), 1);
        assert_eq!(data.alpha(1), 2);
        assert_eq!(data.alpha(8), 0x1D);
        assert_eq!(data.mul(data.alpha(100), data.inverse(data.alpha(100))), 1);
        let system = Field::new(SYSTEM_POLY);
        assert_eq!(system.alpha(8), 0x69);
    }

    #[test]
    fn a_codeword_has_zero_syndromes_and_known_parity() {
        let field = Field::new(DATA_POLY);
        let data: Vec<u8> = (0..DATA_K as u32).map(|i| (i * 7 + 3) as u8).collect();
        let word = codeword(&field, &data, DATA_K);
        assert_eq!(&word[..DATA_K], &data[..]);
        assert_eq!(decode(&field, &word, DATA_K).unwrap(), word);
        // A known vector: the all-zero block has zero parity; a single one
        // at the top gives the generator's coefficients.
        let zero = codeword(&field, &[0u8; DATA_K], DATA_K);
        assert!(zero.iter().all(|&b| b == 0));
        let mut one = [0u8; DATA_K];
        one[DATA_K - 1] = 1;
        let word = codeword(&field, &one, DATA_K);
        let g = field.generator(4, DATA_K);
        assert_eq!(&word[DATA_K..], &[g[3], g[2], g[1], g[0]]);
        let system = Field::new(SYSTEM_POLY);
        let word = codeword(&system, &data[..SYSTEM_K], SYSTEM_K);
        assert_eq!(&word[..SYSTEM_K], &data[..SYSTEM_K]);
    }

    #[test]
    fn two_corrupted_bytes_of_a_data_block_are_recovered() {
        let field = Field::new(DATA_POLY);
        let data: Vec<u8> = (0..DATA_K as u32).map(|i| (i * 31 + 7) as u8).collect();
        let word = codeword(&field, &data, DATA_K);
        for (a, b) in [(0usize, 100usize), (17, 254), (250, 251), (3, 4)] {
            let mut damaged = word;
            damaged[a] ^= 0x5A;
            damaged[b] ^= 0xC3;
            assert_eq!(decode(&field, &damaged, DATA_K).unwrap(), word, "{a}, {b}");
        }
        let mut damaged = word;
        damaged[42] ^= 0xFF;
        assert_eq!(decode(&field, &damaged, DATA_K).unwrap(), word);
    }

    #[test]
    fn interleaving_puts_each_blocks_bytes_a_stride_apart() {
        let field = Field::new(SYSTEM_POLY);
        let data: Vec<u8> = (0..500u32).map(|i| i as u8).collect();
        let out = interleaved(&field, &data, 3, SYSTEM_K);
        assert_eq!(out.len(), 3 * 255);
        let back = crate::r2007::rs_deinterleave(&out, 3, SYSTEM_K);
        assert_eq!(&back[..500], &data[..]);
        assert!(back[500..].iter().all(|&b| b == 0));
    }

    #[test]
    fn a_page_of_encoding_one_keeps_its_data_first() {
        let field = Field::new(DATA_POLY);
        let data: Vec<u8> = (0..300u32).map(|i| (i * 3) as u8).collect();
        let out = data_then_parity(&field, &data, DATA_K);
        assert_eq!(&out[..300], &data[..]);
        assert_eq!(out.len(), 304 + 2 * 4);
    }
}
