// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! PNG screenshots with an own encoder: one fixed-Huffman deflate block with matches against the pixel to the left and
//! above, CRC32 (`oos_hal::crc32`), Adler32; and PPM pictures read back into a canvas.

use super::*;

// CRC-32 is the crate oos-dsp's, where the decoders and the zip reader of oos-store reach it too
pub use oos_dsp::decode::crc32;

// oos-plus: Adler-32 and the deflate tables are the crate oos-store's, whose reader of RIGOL's DHO files inflates
pub use oos_store::{adler32, DIST_BASE, LEN_BASE};

pub(super) fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}


/// Bit writer of a deflate stream: values LSB first, Huffman codes MSB first.
pub(super) struct BitOut {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitOut {
    /// At most 32 bits at a time; whole bytes go out once 32 have gathered.
    fn bits(&mut self, v: u32, n: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += n;
        if self.n >= 32 {
            self.out.extend_from_slice(&(self.acc as u32).to_le_bytes());
            self.acc >>= 32;
            self.n -= 32;
        }
    }
    /// A Huffman code of `n` bits, first bit = most significant.
    fn code(&mut self, c: u32, n: u32) {
        self.bits(c.reverse_bits() >> (32 - n), n);
    }
    /// Literal/length symbol of the fixed code (RFC 1951 3.2.6).
    fn symbol(&mut self, s: u32) {
        match s {
            0..=143 => self.code(0x30 + s, 8),
            144..=255 => self.code(0x190 + s - 144, 9),
            256..=279 => self.code(s - 256, 7),
            _ => self.code(0xc0 + s - 280, 8),
        }
    }
    /// Length symbol and extra bits of a match of 3..=258 bytes, then the 5-bit distance code and its extra bits.
    fn matched(&mut self, len: usize, dist_code: usize, dist_extra: u32) {
        let i = LEN_CODE[len] as usize;
        self.symbol(257 + i as u32);
        self.bits((len - LEN_BASE[i].0 as usize) as u32, LEN_BASE[i].1 as u32);
        self.code(dist_code as u32, 5);
        self.bits(dist_extra, DIST_BASE[dist_code].1 as u32);
    }
    fn finish(mut self) -> Vec<u8> {
        while self.n > 0 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n = self.n.saturating_sub(8);
        }
        self.out
    }
}

/// Index into `LEN_BASE` for each match length 0..=258 (0..2 unused).
pub(super) const LEN_CODE: [u8; 259] = {
    let mut t = [0u8; 259];
    let mut len = 3;
    while len <= 258 {
        let mut i = 28;
        while LEN_BASE[i].0 as usize > len {
            i -= 1;
        }
        t[len] = i as u8;
        len += 1;
    }
    t
};

/// Number of equal bytes at `i` and `i - d`, at most 258 (the longest deflate match); eight bytes per step.
#[inline]
pub(super) fn match_len(raw: &[u8], i: usize, d: usize) -> usize {
    let max = (raw.len() - i).min(258);
    let (a, b) = (&raw[i..i + max], &raw[i - d..i - d + max]);
    // whole words of eight bytes, then the bytes after the last one
    let (wa, wb) = (a.as_chunks::<8>().0, b.as_chunks::<8>().0);
    for (k, (x, y)) in wa.iter().zip(wb).enumerate() {
        let x = u64::from_le_bytes(*x) ^ u64::from_le_bytes(*y);
        if x != 0 {
            return k * 8 + (x.trailing_zeros() / 8) as usize;
        }
    }
    let mut n = wa.len() * 8;
    while n < max && a[n] == b[n] {
        n += 1;
    }
    n
}

/// Deflate (one block, fixed Huffman codes) of picture rows: besides literals only matches against the pixel to the left
/// (distance 3) and the pixel above (distance `stride`). A scope screen is flat areas, grid lines and a few traces, so
/// that is most of what a full LZ77 finds, in one pass, with no hash table and little time on the scope's CPU. The
/// left match costs 10 bits less than the one above (no extra distance bits), so it is looked for first and the one
/// above only when the left one is not the longest possible.
pub(crate) fn deflate_rows(raw: &[u8], stride: usize) -> Vec<u8> {
    deflate_block(raw, stride, true).finish()
}

/// `deflate_rows` as a part of a zlib stream that goes on after it (Tight's, `vnc.rs`): a block that is not the last
/// one, then an empty stored block, which ends on a whole byte (zlib's sync flush): the reader has all of `raw`.
pub(crate) fn deflate_rows_sync(raw: &[u8], stride: usize) -> Vec<u8> {
    let mut o = deflate_block(raw, stride, false);
    o.bits(0, 3); // not the last block, stored
    let mut out = o.finish();
    out.extend_from_slice(&[0, 0, 0xff, 0xff]); // its length, 0, and the complement
    out
}

fn deflate_block(raw: &[u8], stride: usize, last: bool) -> BitOut {
    let (stride_code, stride_extra) = {
        let c = DIST_BASE.iter().rposition(|&(b, _)| b as usize <= stride).unwrap_or(0);
        (c, stride.saturating_sub(DIST_BASE[c].0 as usize) as u32)
    };
    let stride_ok = stride > 3 && stride <= 32768;
    let mut o = BitOut { out: Vec::with_capacity(raw.len() / 8 + 64), acc: 0, n: 0 };
    o.bits(last as u32, 1); // BFINAL
    o.bits(1, 2); // BTYPE 01 = fixed Huffman
    let mut i = 0;
    while i < raw.len() {
        let left = if i >= 3 { match_len(raw, i, 3) } else { 0 };
        let above = if left < 258 && stride_ok && i >= stride { match_len(raw, i, stride) } else { 0 };
        if above > left && above >= 3 {
            o.matched(above, stride_code, stride_extra);
            i += above;
        } else if left >= 3 {
            o.matched(left, 2, 0);
            i += left;
        } else {
            o.symbol(raw[i] as u32);
            i += 1;
        }
    }
    o.symbol(256);
    o
}

/// PNG (8-bit RGB) of XRGB8888 pixels: filter 0 per row, zlib stream of one fixed-Huffman deflate block
/// (`deflate_rows`), so a screen of flat colour and thin lines takes a few percent of its bitmap.
pub fn png(w: u32, h: u32, px: &[u32]) -> Vec<u8> {
    let stride = w as usize * 3 + 1;
    let mut raw = vec![0u8; stride * h as usize];
    for (out, row) in raw.chunks_exact_mut(stride).zip(px.chunks(w as usize)) {
        // out[0] = 0: filter type None
        for (o, &p) in out[1..].as_chunks_mut::<3>().0.iter_mut().zip(row) {
            o.copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut z = vec![0x78, 0x01]; // deflate, 32 K window, no dictionary, (0x7801 % 31 == 0)
    z.extend_from_slice(&deflate_rows(&raw, stride));
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bit, RGB, deflate, filter set 0, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// PNG of the screen canvas.
/// Draws a binary PPM (P6, maxval 255) centred into the canvas (black around it); returns its size.
pub fn ppm_into(b: &[u8], cv: &mut Canvas) -> Option<(i32, i32)> {
    // header: "P6" w h maxval, separated by whitespace, comments with '#'
    let mut fields = Vec::new();
    let mut p = 2;
    if !b.starts_with(b"P6") {
        return None;
    }
    while fields.len() < 3 {
        while p < b.len() && (b[p].is_ascii_whitespace() || b[p] == b'#') {
            if b[p] == b'#' {
                while p < b.len() && b[p] != b'\n' {
                    p += 1;
                }
            }
            p += 1;
        }
        let s = p;
        while p < b.len() && b[p].is_ascii_digit() {
            p += 1;
        }
        // a header that ends inside a comment leaves p one past the end: no field
        fields.push(std::str::from_utf8(b.get(s..p)?).ok()?.parse::<i32>().ok()?);
    }
    let (w, h, max) = (fields[0], fields[1], fields[2]);
    p += 1;
    // the size in usize (64 bits hold 3 * 2^31 * 2^31): in i32 it overflows past 2^31
    if max != 255 || w <= 0 || h <= 0 || b.len() < p + w as usize * h as usize * 3 {
        return None;
    }
    cv.fill(Rect::new(0, 0, cv.w, cv.h), 0);
    let (ox, oy) = ((cv.w - w) / 2, (cv.h - h) / 2);
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (ox + x, oy + y);
            if (0..cv.w).contains(&cx) && (0..cv.h).contains(&cy) {
                let k = p + (y as usize * w as usize + x as usize) * 3;
                cv.px[(cy * cv.w + cx) as usize] = rgb(b[k] as u32, b[k + 1] as u32, b[k + 2] as u32);
            }
        }
    }
    Some((w, h))
}

pub fn png_of(cv: &Canvas) -> Vec<u8> {
    png(cv.w as u32, cv.h as u32, &cv.px)
}
