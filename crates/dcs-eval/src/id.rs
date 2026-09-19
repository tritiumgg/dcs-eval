//! Minting a request id: `<seq>-<tag>`, the shape `publish::is_id`
//! validates and nothing until now produced.
//!
//! The two halves answer two different questions. The `seq` is a
//! zero-padded ten-digit counter, and padding it is the whole point: the
//! executor lists its request directory and answers in *name* order, so a
//! counter that is not padded would have request 10 answered before
//! request 9, and a client that yields replies in id order would be
//! yielding them in an order the wire never promised. The `tag` is eight
//! characters of base36 fixed for the life of one minter, and its job is
//! that two clients sharing one session never pick the same name for two
//! different requests. It guards nothing — a tag is not a secret and
//! nothing on either end of the wire is authorised by one — which is why
//! the generator is written here rather than taken from a dependency.
//! Decision record 0011 is that argument.
//!
//! What seeds a tag is four terms mixed together: this process's id, the
//! nanoseconds since the epoch, the address of a stack local, and a
//! process-local counter bumped on every construction. The counter is
//! what makes two minters built in one process distinct *by mechanism*
//! rather than by luck — built back to back in a loop they share the
//! process id and, built in the same stack frame, the same address, so
//! without it distinctness would rest entirely on the clock resolving two
//! adjacent calls apart. The counter buys nothing across processes; that
//! is what the process id and the clock are for, and no test here can
//! speak for two processes started in the same millisecond.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The last `seq` that fits in ten digits. Past it there is no id of the
/// right shape to mint, and a minter says so rather than producing one
/// `is_id` would refuse or, worse, an eleven-digit name that sorts before
/// every ten-digit one.
const LAST_SEQ: u64 = 9_999_999_999;

/// The alphabet a tag is written in, and its length is the base.
const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// How many characters a tag has. Eight sits inside the four-to-twelve
/// window `is_id` admits with room on both sides, and 36^8 is about
/// 2.8e12 — enough that two clients on one session picking the same tag
/// is not a thing that happens, which is all the tag is asked for.
const TAG_LEN: u32 = 8;

/// One draw as exactly [`TAG_LEN`] characters of base36.
///
/// The draw is reduced modulo 36^8 first and the result is left-padded
/// with `0`, and both steps are load-bearing. A naive encoder run on a
/// small draw gives one or two characters, which is under the shortest
/// tag `is_id` admits; run on a draw near the top of a `u64` it gives
/// thirteen, which is over the longest. Fixing the width is what makes
/// every draw produce a name the publisher will take.
fn tag_of(draw: u64) -> String {
    let span = u64::from(DIGITS.len() as u32).pow(TAG_LEN);
    let mut value = draw % span;
    let mut out = vec![b'0'; TAG_LEN as usize];
    let mut at = TAG_LEN as usize;
    while value > 0 && at > 0 {
        at -= 1;
        out[at] = DIGITS[(value % DIGITS.len() as u64) as usize];
        value /= DIGITS.len() as u64;
    }
    String::from_utf8(out).expect("the alphabet is ASCII")
}

/// SplitMix64: one seed in, one well-mixed word out. It is here because
/// the seed's four terms are each nearly constant between two adjacent
/// constructions — the same process id, the same address, a counter one
/// apart — and a tag taken straight off such a seed would differ in one
/// low character. The mixer spreads a one-bit difference across the
/// whole word, which is exactly what is wanted: adjacent seeds, unrelated
/// tags.
fn mix(seed: u64) -> u64 {
    let state = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// How many minters this process has built. Mixed into every seed so
/// that two built in one process differ whatever the clock did.
static BUILT: AtomicU64 = AtomicU64::new(0);

/// A per-client counter and the tag it puts on every id it mints.
#[derive(Debug, Clone)]
pub struct Minter {
    seq: u64,
    tag: String,
}

impl Minter {
    /// A minter seeded from this process and this moment: the process id,
    /// the nanoseconds since the epoch, the address of a stack local, and
    /// the count of minters built here so far. The module header says
    /// what each term is for.
    #[must_use]
    pub fn new() -> Self {
        let here = 0u8;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos() as u64);
        let seed = u64::from(std::process::id())
            ^ nanos.rotate_left(17)
            ^ (&raw const here as u64).rotate_left(33)
            ^ BUILT
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        Self::seeded(seed)
    }

    /// A minter whose tag is `seed`'s, counting from one. The generator
    /// is deterministic, so this is how a test pins a tag.
    #[must_use]
    pub fn seeded(seed: u64) -> Self {
        Self::seeded_at(seed, 1)
    }

    /// The constructor that names both halves: the tag's seed and the
    /// `seq` the next id carries. A caller resuming a counter across a
    /// restart uses it, and so does a test that wants ids near a boundary
    /// without minting ten million of them.
    #[must_use]
    pub fn seeded_at(seed: u64, seq: u64) -> Self {
        Self {
            seq,
            tag: tag_of(mix(seed)),
        }
    }

    /// The tag every id from this minter carries.
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The next id, or `None` once the counter has run past ten digits.
    /// A minter that has run out is not an error a caller can retry: the
    /// answer is a new minter with a new tag, which sorts wherever its
    /// counter says and collides with nothing.
    pub fn mint(&mut self) -> Option<String> {
        if self.seq > LAST_SEQ {
            return None;
        }
        let id = format!("{:010}-{}", self.seq, self.tag);
        self.seq += 1;
        Some(id)
    }
}

impl Default for Minter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::is_id;
    use std::collections::HashSet;

    #[test]
    fn every_minted_id_is_one_publish_would_take() {
        // Over many *minters* rather than many ids of one, because one
        // minter exercises exactly one tag and a broken encoder would
        // have to be unlucky to be caught by it.
        for _ in 0..200 {
            let mut m = Minter::new();
            let id = m.mint().expect("a fresh minter mints");
            assert!(is_id(&id), "{id} is not a shape the publisher takes");
        }
    }

    #[test]
    fn a_tag_is_eight_characters_for_every_draw() {
        // The encoder directly, at both ends and either side of a digit
        // boundary: a small draw is where a naive encoder is too short
        // and a huge one is where it is too long.
        for draw in [0, 1, 35, 36, 36u64.pow(8) - 1, u64::MAX] {
            let tag = tag_of(draw);
            assert_eq!(tag.len(), 8, "draw {draw} encoded as {tag}");
            assert!(
                tag.bytes().all(|b| b.is_ascii_alphanumeric()),
                "draw {draw} encoded as {tag}"
            );
            assert!(
                is_id(&format!("0000000001-{tag}")),
                "draw {draw} makes an id the publisher refuses: {tag}"
            );
        }
    }

    #[test]
    fn one_minters_ids_sort_in_publication_order() {
        // Across a power-of-ten boundary, which is the only place an
        // unpadded counter and a padded one disagree.
        let mut m = Minter::seeded_at(7, 9_999_998);
        let minted: Vec<String> = (0..20).map(|_| m.mint().expect("a mint")).collect();
        let mut sorted = minted.clone();
        sorted.sort();
        assert_eq!(minted, sorted, "name order is publication order");
    }

    #[test]
    fn one_minter_puts_the_same_tag_on_every_id() {
        let mut m = Minter::new();
        let tags: HashSet<String> = (0..50)
            .map(|_| {
                let id = m.mint().expect("a mint");
                id.split_once('-').expect("an id has a dash").1.to_owned()
            })
            .collect();
        assert_eq!(tags.len(), 1, "one minter, one tag: {tags:?}");
    }

    #[test]
    fn two_minters_in_one_process_do_not_share_a_tag() {
        // This rests on the process-local counter in the seed and on
        // nothing else: these minters share a process id, and being built
        // in one loop they share the address of the stack local too, so
        // the counter is the only term that has to differ. It says
        // nothing whatever about two *processes* started in the same
        // millisecond, which nothing in this crate can test.
        let tags: HashSet<String> = (0..1_000).map(|_| Minter::new().tag().to_owned()).collect();
        assert_eq!(tags.len(), 1_000, "1,000 minters, 1,000 tags");
    }

    #[test]
    fn a_seeded_minter_is_the_same_generator_every_run() {
        // The golden value this crate's mixer and encoder actually print.
        // It pins this generator so a change to either is visible, and
        // claims nothing about any other implementation of either.
        assert_eq!(Minter::seeded(1).tag(), "0aytrp35");
        assert_eq!(
            Minter::seeded(1).mint().expect("a mint"),
            "0000000001-0aytrp35"
        );
    }

    #[test]
    fn a_minter_refuses_past_the_last_ten_digit_seq() {
        let mut m = Minter::seeded_at(3, LAST_SEQ);
        let last = m.mint().expect("the last ten-digit seq mints");
        assert_eq!(&last[..10], "9999999999");
        assert!(m.mint().is_none(), "and there is no eleven-digit id");
        assert!(m.mint().is_none(), "still none when asked again");
    }
}
