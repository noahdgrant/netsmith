//! A reader of the compound file (OLE, `[MS-CFB]`) that holds an Altium library or board: a
//! small file system of storages (folders) and streams (files) in one file of sectors.
//!
//! Only what reading needs: the header, the allocation table (with its DIFAT chain), the
//! directory, the mini stream for the streams under the cutoff, and the tree of each storage's
//! children. A chain that loops or a sector past the end of the file is an error, never a hang.

/// The first eight bytes of every compound file.
pub(crate) const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FREE: u32 = 0xFFFF_FFFF;
const NO_ENTRY: u32 = 0xFFFF_FFFF;
const MINI_SECTOR: usize = 64;

/// What a directory entry holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Storage,
    Stream,
    Root,
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    kind: Option<Kind>,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

/// A compound file read into memory.
#[derive(Debug)]
pub(crate) struct Compound<'a> {
    bytes: &'a [u8],
    sector: usize,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_stream: Vec<u8>,
    cutoff: u64,
    entries: Vec<Entry>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("four bytes"))
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("eight bytes"))
}

impl<'a> Compound<'a> {
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 512 || bytes[..8] != SIGNATURE {
            return Err("not a compound (OLE) file".into());
        }
        let major = u16_at(bytes, 26);
        let shift = u16_at(bytes, 30);
        let sector = match (major, shift) {
            (3, 9) => 512,
            (4, 12) => 4096,
            _ => {
                return Err(format!(
                    "a compound file of version {major} with sectors of 2^{shift} bytes, which netsmith does not read"
                ));
            }
        };
        let mini_shift = u16_at(bytes, 32);
        if mini_shift != 6 {
            return Err(format!(
                "a compound file with mini sectors of 2^{mini_shift} bytes"
            ));
        }
        let fat_sectors = u32_at(bytes, 44) as usize;
        let first_dir = u32_at(bytes, 48);
        let cutoff = u64::from(u32_at(bytes, 56));
        let first_mini_fat = u32_at(bytes, 60);
        let mut difat_next = u32_at(bytes, 68);
        let mut compound = Compound {
            bytes,
            sector,
            fat: Vec::new(),
            mini_fat: Vec::new(),
            mini_stream: Vec::new(),
            cutoff,
            entries: Vec::new(),
        };
        let max_sectors = bytes.len() / sector;
        // The sectors of the allocation table: 109 in the header, the rest in the DIFAT chain.
        let mut fat_ids: Vec<u32> = (0..109).map(|i| u32_at(bytes, 76 + 4 * i)).collect();
        let mut hops = 0;
        while difat_next != END_OF_CHAIN && difat_next != FREE {
            hops += 1;
            if hops > max_sectors {
                return Err("the compound file's DIFAT chain loops".into());
            }
            let s = compound.sector_bytes(difat_next)?;
            let per = sector / 4 - 1;
            fat_ids.extend((0..per).map(|i| u32_at(s, 4 * i)));
            difat_next = u32_at(s, 4 * per);
        }
        let fat_ids: Vec<u32> = fat_ids
            .into_iter()
            .filter(|&id| id != FREE && id != END_OF_CHAIN)
            .take(fat_sectors)
            .collect();
        let mut fat = Vec::with_capacity(fat_ids.len() * sector / 4);
        for id in fat_ids {
            let s = compound.sector_bytes(id)?;
            fat.extend((0..sector / 4).map(|i| u32_at(s, 4 * i)));
        }
        compound.fat = fat;

        let dir = compound.chain(first_dir, None)?;
        for raw in dir.as_chunks::<128>().0 {
            let len = (u16_at(raw, 64) as usize).min(64);
            let units: Vec<u16> = (0..len.saturating_sub(2) / 2)
                .map(|i| u16_at(raw, 2 * i))
                .collect();
            let kind = match raw[66] {
                1 => Some(Kind::Storage),
                2 => Some(Kind::Stream),
                5 => Some(Kind::Root),
                _ => None,
            };
            let mut size = u64_at(raw, 120);
            if major == 3 {
                // A version 3 file gives the size in 32 bits; the high ones may be anything.
                size &= 0xFFFF_FFFF;
            }
            compound.entries.push(Entry {
                name: String::from_utf16_lossy(&units),
                kind,
                left: u32_at(raw, 68),
                right: u32_at(raw, 72),
                child: u32_at(raw, 76),
                start: u32_at(raw, 116),
                size,
            });
        }
        let Some(root) = compound.entries.first().cloned() else {
            return Err("the compound file has no directory".into());
        };
        if root.kind != Some(Kind::Root) {
            return Err("the compound file's first directory entry is not its root".into());
        }
        if first_mini_fat != END_OF_CHAIN && first_mini_fat != FREE {
            let raw = compound.chain(first_mini_fat, None)?;
            compound.mini_fat = raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
        }
        if root.size > 0 {
            compound.mini_stream = compound.chain(root.start, Some(root.size))?;
        }
        Ok(compound)
    }

    fn sector_bytes(&self, id: u32) -> Result<&'a [u8], String> {
        let start = (id as usize + 1) * self.sector;
        self.bytes
            .get(start..start + self.sector)
            .ok_or_else(|| format!("the compound file names sector {id}, past its end"))
    }

    /// The bytes of the chain of regular sectors that starts at `start`, cut at `size`.
    fn chain(&self, start: u32, size: Option<u64>) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut id = start;
        let mut hops = 0;
        while id != END_OF_CHAIN {
            hops += 1;
            if hops > self.fat.len() + 1 {
                return Err("a chain of the compound file loops".into());
            }
            out.extend_from_slice(self.sector_bytes(id)?);
            if size.is_some_and(|s| out.len() as u64 >= s) {
                break;
            }
            id = *self
                .fat
                .get(id as usize)
                .ok_or_else(|| format!("sector {id} is past the allocation table"))?;
        }
        if let Some(size) = size {
            if (out.len() as u64) < size {
                return Err("a stream of the compound file is shorter than its size".into());
            }
            out.truncate(size as usize);
        }
        Ok(out)
    }

    fn mini_chain(&self, start: u32, size: u64) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut id = start;
        let mut hops = 0;
        while id != END_OF_CHAIN && (out.len() as u64) < size {
            hops += 1;
            if hops > self.mini_fat.len() + 1 {
                return Err("a chain of the compound file's mini stream loops".into());
            }
            let at = id as usize * MINI_SECTOR;
            let bytes = self
                .mini_stream
                .get(at..at + MINI_SECTOR)
                .ok_or_else(|| format!("mini sector {id} is past the mini stream"))?;
            out.extend_from_slice(bytes);
            id = *self
                .mini_fat
                .get(id as usize)
                .ok_or_else(|| format!("mini sector {id} is past the mini allocation table"))?;
        }
        if (out.len() as u64) < size {
            return Err("a stream of the compound file is shorter than its size".into());
        }
        out.truncate(size as usize);
        Ok(out)
    }

    /// The children of the storage `id` (0 is the root), as (name, id, kind).
    pub(crate) fn children(&self, id: usize) -> Vec<(String, usize, Kind)> {
        let mut out = Vec::new();
        let Some(parent) = self.entries.get(id) else {
            return out;
        };
        let mut stack = vec![parent.child];
        let mut seen = vec![false; self.entries.len()];
        while let Some(at) = stack.pop() {
            if at == NO_ENTRY {
                continue;
            }
            let at = at as usize;
            if at >= self.entries.len() || seen[at] {
                continue;
            }
            seen[at] = true;
            let e = &self.entries[at];
            if let Some(kind) = e.kind {
                out.push((e.name.clone(), at, kind));
            }
            stack.push(e.left);
            stack.push(e.right);
        }
        out.sort_by_key(|a| a.1);
        out
    }

    /// The entry at `path` from the root, its names matched without regard to case as the
    /// format says.
    pub(crate) fn find(&self, path: &[&str]) -> Option<usize> {
        let mut at = 0;
        for name in path {
            at = self
                .children(at)
                .into_iter()
                .find(|(n, _, _)| n.eq_ignore_ascii_case(name))?
                .1;
        }
        Some(at)
    }

    /// The bytes of the stream `id`.
    pub(crate) fn stream(&self, id: usize) -> Result<Vec<u8>, String> {
        let e = &self.entries[id];
        if e.kind != Some(Kind::Stream) {
            return Err(format!("`{}` is not a stream", e.name));
        }
        if e.size == 0 {
            return Ok(Vec::new());
        }
        if e.size < self.cutoff {
            self.mini_chain(e.start, e.size)
        } else {
            self.chain(e.start, Some(e.size))
        }
    }

    /// The bytes of the stream at `path`, `None` when there is none.
    pub(crate) fn read(&self, path: &[&str]) -> Result<Option<Vec<u8>>, String> {
        match self.find(path) {
            Some(id) if self.entries[id].kind == Some(Kind::Stream) => self.stream(id).map(Some),
            _ => Ok(None),
        }
    }
}
