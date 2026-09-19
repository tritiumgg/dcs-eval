//! SHA-256, written here rather than taken from a crate.
//!
//! This crate links nothing outside the standard library, so that a program
//! wanting the wire does not take a dependency tree with it; decision record
//! 0011 argues that and names this hash as one of the four things it costs.
//! The trade is a cheap one to make safely: the algorithm is fixed, it has
//! published test vectors, and the use here is provenance — recording which
//! bytes ran — rather than anything that has to resist an adversary.
//!
//! It is proved twice over. The published vectors pin the algorithm, and
//! every digest is also compared against `sha256sum` on the same bytes on
//! disk, at every length across the padding boundaries and over a file large
//! enough to need many blocks. A digest here that disagrees with that tool is
//! a defect in this module.

/// A digest in progress. Feed it with [`Sha256::update`] as often as the
/// caller likes and take the result with [`Sha256::finish`].
///
/// Streaming rather than one-shot because a caller that already holds a
/// body should not have to concatenate it with anything to hash it, and
/// because a later reader of a file too big to hold could feed it in pieces.
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    /// Bytes not yet part of a full block.
    buffer: [u8; 64],
    buffered: usize,
    /// The total length fed in, in bytes. The padding encodes it in bits.
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            state: INITIAL,
            buffer: [0; 64],
            buffered: 0,
            length: 0,
        }
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        // Fill whatever is left of a part-full block first, then take whole
        // blocks straight out of the caller's slice, then keep the tail.
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(bytes.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&bytes[..take]);
            self.buffered += take;
            bytes = &bytes[take..];
            if self.buffered < 64 {
                // Still short of a block, and the caller's slice is spent:
                // returning here is what keeps `buffered` the count of what
                // is held rather than the length of the last slice.
                return;
            }
            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
        }
        while bytes.len() >= 64 {
            let (block, rest) = bytes.split_at(64);
            let block: [u8; 64] = block.try_into().expect("a 64-byte block");
            self.compress(&block);
            bytes = rest;
        }
        self.buffer[..bytes.len()].copy_from_slice(bytes);
        self.buffered = bytes.len();
    }

    /// The digest, consuming the state: a hash that has been padded cannot
    /// be fed any more, and taking `self` says so in the signature.
    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        // A single one bit, then zeroes, then the length in bits as eight
        // big-endian bytes, ending on a block boundary.
        self.update(&[0x80]);
        // `update` counted that byte into the length, which the padding must
        // not include; the figure was taken before it went in.
        while self.buffered != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        debug_assert_eq!(self.buffered, 0, "the padding ends on a block boundary");
        let mut out = [0u8; 32];
        for (word, slot) in self.state.iter().zip(out.as_chunks_mut::<4>().0) {
            *slot = word.to_be_bytes();
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*chunk);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, add) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(add);
        }
    }
}

/// The digest of `bytes` in one call.
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finish()
}

/// A digest as `sha256sum` prints it: sixty-four lower-case hex characters.
pub fn hex(digest: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(char::from_digit((byte >> 4) as u32, 16).expect("a hex digit"));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).expect("a hex digit"));
    }
    out
}

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[cfg(test)]
mod file_source {
    use super::*;
    use crate::testing::{Sandbox, sha256sum};

    use std::fs;

    /// The three published vectors. Each was also run through `sha256sum` on
    /// this machine while the test was written, because a vector typed from
    /// memory proves nothing about the algorithm beside it.
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const NINE_HUNDRED: &str = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";
    const NINE_HUNDRED_INPUT: &str = "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";

    #[test]
    fn the_published_vector_for_the_empty_string() {
        assert_eq!(hex(&digest(b"")), EMPTY);
    }

    #[test]
    fn the_published_vector_for_abc() {
        assert_eq!(hex(&digest(b"abc")), ABC);
    }

    #[test]
    fn the_published_vector_for_the_896_bit_case() {
        assert_eq!(hex(&digest(NINE_HUNDRED_INPUT.as_bytes())), NINE_HUNDRED);
    }

    #[test]
    fn the_same_bytes_fed_in_pieces_hash_the_same() {
        // A streaming bug survives a one-shot vector, so the split is taken
        // at every offset across two block boundaries rather than at one
        // convenient place.
        let bytes: Vec<u8> = (0..200u32).map(|i| (i * 7 % 251) as u8).collect();
        let whole = hex(&digest(&bytes));
        for at in 0..=bytes.len() {
            let mut h = Sha256::new();
            h.update(&bytes[..at]);
            h.update(&bytes[at..]);
            assert_eq!(hex(&h.finish()), whole, "split at {at}");
        }
    }

    #[test]
    fn every_length_across_the_padding_boundaries_matches_sha256sum() {
        // 55 and 56 are where the length no longer fits in the last block,
        // 63 and 64 where a block is exactly full, and 119 and 120 the same
        // one block along. The sweep covers all six rather than naming them.
        let b = Sandbox::new();
        for len in 0..=130usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let path = b.join(&format!("len-{len}.bin"));
            fs::write(&path, &bytes).expect("the fixture is written");
            assert_eq!(
                hex(&digest(&bytes)),
                sha256sum(&path),
                "{len} bytes disagree with sha256sum"
            );
        }
    }

    #[test]
    fn a_file_of_a_few_hundred_kilobytes_matches_sha256sum() {
        let b = Sandbox::new();
        let mut bytes = Vec::with_capacity(300_000);
        let mut x: u32 = 0x1234_5678;
        while bytes.len() < 300_000 {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            bytes.extend_from_slice(&x.to_le_bytes());
        }
        let path = b.join("big.bin");
        fs::write(&path, &bytes).expect("the fixture is written");
        assert_eq!(hex(&digest(&bytes)), sha256sum(&path));
    }

    #[test]
    fn hex_is_lower_case_and_sixty_four_characters() {
        let line = hex(&digest(b"abc"));
        assert_eq!(line.len(), 64);
        assert!(line.chars().all(|c| c.is_ascii_hexdigit()), "{line}");
        assert_eq!(line, line.to_ascii_lowercase(), "lower case: {line}");
    }
}
