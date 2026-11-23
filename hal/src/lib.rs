// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! OOS hardware layer.
//!
//! The crate root holds what does not depend on one board: the model and its limits (`model`), the vendor file keys
//! (`vendorkeys`), signed updates (`update`), WLAN (`wlan`) and the start-up status file (`bootstatus`). Everything of
//! the DHO800/DHO900 board (RK3399 + Zynq FPGA over PCIe/XDMA) is in `dho`; a port to another board family adds a
//! module next to it (docs/porting.md).
//!
//! The register sequences are described in `docs/protocols/`. No code of the vendor libraries is used. Functions that
//! build register sequences are pure (they return `(offset, value)` lists and update software shadows of the
//! write-only registers), so they are unit-tested on the host; `dho::bus` applies them to the hardware.

pub mod bluez;
pub mod bootstatus;
pub mod dbus;
pub mod fsutil;
#[cfg(test)]
mod hostile;
pub mod model;
pub mod procs;
pub mod target;
pub mod update;
pub mod vendorkeys;
pub mod wlan;

pub mod dho;

/// The `N` bytes of `b` from offset `o` on, for `from_le_bytes`. Panics past the end of `b` like indexing does: for
/// records whose length was checked when they were read in.
pub fn bytes_at<const N: usize>(b: &[u8], o: usize) -> [u8; N] {
    std::array::from_fn(|k| b[o + k])
}

/// `bytes_at`, or `None` when they are not all in `b`: for bytes from outside (a packet, a request) not checked yet.
pub fn try_bytes_at<const N: usize>(b: &[u8], o: usize) -> Option<[u8; N]> {
    b.get(o..o.checked_add(N)?)?.try_into().ok()
}

/// The `key=value` lines of a text, each as `cut` (a file's rule for comments) leaves it, blank ones and those starting
/// with `#` after blanks left out: (line number from 1, the line, key and value as written around its first `=`).
pub fn key_value_lines<'a>(
    text: &'a str,
    cut: impl Fn(&'a str) -> &'a str,
) -> impl Iterator<Item = (usize, &'a str, Option<(&'a str, &'a str)>)> {
    let lines = text.lines().map(cut).enumerate();
    let lines = lines.filter(|(_, l)| l.trim_start().chars().next().is_some_and(|c| c != '#'));
    lines.map(|(n, l)| (n + 1, l, l.split_once('=')))
}

/// CRC-32 tables for eight bytes per step ("slicing by 8"): `T[0]` is the usual byte table, `T[k][n]` the CRC of byte
/// `n` followed by `k` zero bytes.
const CRC_T: [[u32; 256]; 8] = {
    let mut t = [[0u32; 256]; 8];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[0][n] = c;
        n += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut n = 0;
        while n < 256 {
            let c = t[k - 1][n];
            t[k][n] = t[0][(c & 0xff) as usize] ^ (c >> 8);
            n += 1;
        }
        k += 1;
    }
    t
};

/// zlib / IEEE 802.3 / PNG CRC32, eight bytes per step.
pub fn crc32(data: &[u8]) -> u32 {
    let t = &CRC_T;
    let mut c = !0u32;
    let (blocks, rest) = data.as_chunks::<8>();
    for b in blocks {
        let lo = c ^ u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        c = t[7][(lo & 0xff) as usize]
            ^ t[6][(lo >> 8 & 0xff) as usize]
            ^ t[5][(lo >> 16 & 0xff) as usize]
            ^ t[4][(lo >> 24) as usize]
            ^ t[3][b[4] as usize]
            ^ t[2][b[5] as usize]
            ^ t[1][b[6] as usize]
            ^ t[0][b[7] as usize];
    }
    !rest.iter().fold(c, |c, &b| t[0][((c ^ b as u32) & 0xff) as usize] ^ (c >> 8))
}

/// xorshift64 (13, 7, 17): the same sequence for the same seed, for the demo signals (the pictures and tests depend on
/// it) and the start of the mDNS probes. Not for secrets.
pub struct Rng(u64);

impl Rng {
    /// `seed` with its lowest bit set: never the all-zero state.
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// In [0, 1): the top 53 bits of `next_u64`.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Approximate standard normal (sum of 4 uniforms).
    pub fn gauss(&mut self) -> f32 {
        let s = self.uniform() + self.uniform() + self.uniform() + self.uniform();
        ((s - 2.0) * 1.7320508) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    // Break it catches: another shift, seed or scaling in the one generator, which would change the demo pictures
    // and the mDNS probe times. The generator it replaced (wave::Rng, mdns Responder::rand) is written out here.
    #[test]
    fn rng_gives_the_sequence_of_the_generators_it_replaced() {
        let mut r = Rng::new(0x9e37_79b9_7f4a_7c15);
        assert_eq!(
            [r.next_u64(), r.next_u64(), r.next_u64()],
            [0xdc1b_77ae_0bf3_4dad, 0x64f0_eeb9_026e_6076, 0x7b07_ce91_e590_6136]
        );
        for seed in [0, 1, 2, 17, u64::MAX] {
            let (mut old, mut a, mut b) = (seed | 1, Rng::new(seed), Rng::new(seed));
            for _ in 0..10_000 {
                old ^= old << 13;
                old ^= old >> 7;
                old ^= old << 17;
                assert_eq!((a.next_u64(), b.uniform()), (old, (old >> 11) as f64 / (1u64 << 53) as f64), "{:#x}", seed);
            }
        }
    }
}
