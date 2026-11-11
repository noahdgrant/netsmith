//! A small PNG writer with no dependency: 8-bit grayscale with stored (uncompressed) deflate
//! blocks, as the Gerber cross-check's debug pictures have always been written
//! (`netsmith-fab`'s raster), and 8-bit RGBA compressed with netsmith's own deflate, for the 3D
//! renders (`netsmith-pcb`'s `raster3d`).
//!
//! Both are deterministic: the same pixels give the same bytes on every machine. The
//! compressor is LZ77 over a 32 KiB window (a hash of three bytes and a chain of at most
//! [`MAX_CHAIN`] candidates, greedy matching) coded with deflate's fixed Huffman codes, in one
//! block, with no filter on the scanlines. It is not as small as zlib's best, but a render's
//! large flat areas shrink to a few percent of the raw pixels.

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = crc_table();

/// The CRC-32 of PNG and zlib (polynomial 0xEDB88320).
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// The Adler-32 checksum that ends a zlib stream.
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// The scanlines of an image, each after its filter byte (0, none).
fn scanlines(row_bytes: usize, height: usize, pixels: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((row_bytes + 1) * height);
    for row in pixels.chunks(row_bytes.max(1)).take(height) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    raw
}

fn file(width: usize, height: usize, colour_type: u8, zlib: &[u8]) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    // 8 bit, the colour type, deflate, filter 0, no interlace.
    ihdr.extend_from_slice(&[8, colour_type, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A grayscale PNG of `width` x `height` pixels, `pixels` in row order, top row first, with
/// stored (uncompressed) deflate blocks.
pub fn gray(width: usize, height: usize, pixels: &[u8]) -> Vec<u8> {
    let raw = scanlines(width, height, pixels);
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    if blocks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    file(width, height, 0, &z)
}

/// An RGBA PNG of `width` x `height` pixels, `rgba` four bytes a pixel (red, green, blue,
/// alpha) in row order, top row first, compressed (see the module's header).
pub fn rgba(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    let raw = scanlines(width * 4, height, rgba);
    let mut z = vec![0x78, 0x01];
    z.extend_from_slice(&deflate(&raw));
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    file(width, height, 6, &z)
}

/// The most candidates the compressor looks at for a match.
pub const MAX_CHAIN: usize = 48;
const WINDOW: usize = 32768;
const HASH_BITS: u32 = 15;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    /// Writes the low `n` bits of `v`, least significant first.
    fn put(&mut self, v: u32, n: u32) {
        self.acc |= u64::from(v) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// Writes a Huffman code of `n` bits, most significant first.
    fn code(&mut self, code: u32, n: u32) {
        self.put(code.reverse_bits() >> (32 - n), n);
    }

    fn literal(&mut self, sym: u16) {
        match sym {
            0..=143 => self.code(0x30 + u32::from(sym), 8),
            144..=255 => self.code(0x190 + u32::from(sym - 144), 9),
            256..=279 => self.code(u32::from(sym - 256), 7),
            _ => self.code(0xC0 + u32::from(sym - 280), 8),
        }
    }

    fn matched(&mut self, len: usize, dist: usize) {
        let li = LENGTH_BASE.partition_point(|&b| usize::from(b) <= len) - 1;
        self.literal(257 + li as u16);
        self.put(
            (len - usize::from(LENGTH_BASE[li])) as u32,
            u32::from(LENGTH_EXTRA[li]),
        );
        let di = DIST_BASE.partition_point(|&b| usize::from(b) <= dist) - 1;
        self.code(di as u32, 5);
        self.put(
            (dist - usize::from(DIST_BASE[di])) as u32,
            u32::from(DIST_EXTRA[di]),
        );
    }
}

fn hash(data: &[u8], i: usize) -> usize {
    let v = u32::from(data[i]) | (u32::from(data[i + 1]) << 8) | (u32::from(data[i + 2]) << 16);
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// Raw deflate (no zlib header) of `data`: one final block with the fixed Huffman codes.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits {
        out: Vec::with_capacity(data.len() / 8 + 16),
        acc: 0,
        n: 0,
    };
    bits.put(1, 1); // BFINAL
    bits.put(1, 2); // BTYPE = 01, fixed Huffman codes
    const NONE: u32 = u32::MAX;
    let mut head = vec![NONE; 1 << HASH_BITS];
    let mut prev = vec![NONE; WINDOW];
    let insert = |head: &mut [u32], prev: &mut [u32], i: usize| {
        if i + MIN_MATCH <= data.len() {
            let h = hash(data, i);
            prev[i % WINDOW] = head[h];
            head[h] = i as u32;
        }
    };
    let mut i = 0;
    while i < data.len() {
        let mut best_len = 0;
        let mut best_dist = 0;
        if i + MIN_MATCH <= data.len() {
            let max = (data.len() - i).min(MAX_MATCH);
            let mut cand = head[hash(data, i)];
            let mut chain = 0;
            while cand != NONE && chain < MAX_CHAIN {
                let c = cand as usize;
                if i - c > WINDOW - 1 {
                    break;
                }
                if data[c + best_len.min(max - 1)] == data[i + best_len.min(max - 1)] {
                    let mut l = 0;
                    while l < max && data[c + l] == data[i + l] {
                        l += 1;
                    }
                    if l > best_len {
                        best_len = l;
                        best_dist = i - c;
                        if l == max {
                            break;
                        }
                    }
                }
                let next = prev[c % WINDOW];
                // A chain entry overwritten by a newer position is not older than `c`.
                if next != NONE && next as usize >= c {
                    break;
                }
                cand = next;
                chain += 1;
            }
        }
        if best_len >= MIN_MATCH {
            bits.matched(best_len, best_dist);
            for k in i..i + best_len {
                insert(&mut head, &mut prev, k);
            }
            i += best_len;
        } else {
            bits.literal(u16::from(data[i]));
            insert(&mut head, &mut prev, i);
            i += 1;
        }
    }
    bits.literal(256);
    if bits.n > 0 {
        bits.out.push(bits.acc as u8);
    }
    bits.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn inflate(z: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(z)
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    fn idat(png: &[u8]) -> Vec<u8> {
        let mut at = 8;
        let mut z = Vec::new();
        while at < png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            assert_eq!(
                u32::from_be_bytes(png[at + 8 + len..at + 12 + len].try_into().unwrap()),
                crc32(&png[at + 4..at + 8 + len])
            );
            if &png[at + 4..at + 8] == b"IDAT" {
                z.extend_from_slice(&png[at + 8..at + 8 + len]);
            }
            at += 12 + len;
        }
        z
    }

    #[test]
    fn deflate_round_trips_through_an_independent_inflater() {
        let mut cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![7],
            b"abcabcabcabcabcabc".to_vec(),
            vec![0; 100_000],
        ];
        // A pseudo-random stream with repeats at many distances and lengths.
        let mut x = 12345u32;
        let mut noisy = Vec::new();
        for i in 0..200_000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            if (x >> 16).is_multiple_of(7) && i > 40_000 {
                let back = ((x >> 3) as usize % 40_000) + 1;
                let len = (x as usize >> 20) % 300;
                for _ in 0..len {
                    noisy.push(noisy[noisy.len() - back]);
                }
            } else {
                noisy.push((x >> 24) as u8);
            }
        }
        cases.push(noisy);
        for data in cases {
            let mut z = vec![0x78, 0x01];
            z.extend_from_slice(&deflate(&data));
            z.extend_from_slice(&adler32(&data).to_be_bytes());
            assert_eq!(inflate(&z), data, "{} bytes", data.len());
        }
    }

    #[test]
    fn an_rgba_png_holds_its_pixels_and_compresses_flat_areas() {
        let (w, h) = (300, 200);
        let mut px = vec![0u8; w * h * 4];
        for (i, p) in px.chunks_mut(4).enumerate() {
            let (x, y) = (i % w, i / w);
            p.copy_from_slice(&[(x / 50) as u8 * 40, (y / 50) as u8 * 60, 200, 255]);
        }
        let png = rgba(w, h, &px);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(png[25], 6, "colour type RGBA");
        let raw = inflate(&idat(&png));
        let mut back = Vec::new();
        for row in raw.chunks(w * 4 + 1) {
            assert_eq!(row[0], 0);
            back.extend_from_slice(&row[1..]);
        }
        assert_eq!(back, px);
        assert!(png.len() < px.len() / 20, "{} bytes", png.len());
        assert_eq!(png, rgba(w, h, &px));
    }
}
