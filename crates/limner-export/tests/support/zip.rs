//! A zip reader that accepts only what a USDZ package may be: stored, unencrypted
//! files whose data starts on a 64-byte boundary. It shares no code with the writer,
//! down to the CRC, so the two can only agree by both being right.

pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
    /// Where the local header starts in the archive.
    pub header_offset: usize,
    /// Where the file's bytes start in the archive.
    pub data_offset: usize,
    /// The local header's extra field, which carries the alignment padding.
    pub extra: Vec<u8>,
}

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;

fn u16_at(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// CRC-32 as zip defines it (reflected, polynomial 0xEDB88320), one bit at a time.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Reads a USDZ package, checking every rule of the USDZ specification that concerns
/// the container. Panics with the rule that was broken.
pub fn read_usdz(bytes: &[u8]) -> Vec<ZipEntry> {
    assert!(
        bytes.len() >= 22,
        "room for the end-of-central-directory record"
    );
    let end = bytes.len() - 22;
    assert_eq!(
        u32_at(bytes, end),
        END,
        "the archive ends with the end record, no comment"
    );
    assert_eq!(u16_at(bytes, end + 4), 0, "one disk");
    assert_eq!(u16_at(bytes, end + 6), 0, "one disk");
    let count = u16_at(bytes, end + 8);
    assert_eq!(u16_at(bytes, end + 10), count, "one disk holds every entry");
    let directory_size = u32_at(bytes, end + 12) as usize;
    let directory_offset = u32_at(bytes, end + 16) as usize;
    assert_eq!(u16_at(bytes, end + 20), 0, "no archive comment");
    assert_eq!(
        directory_offset + directory_size,
        end,
        "the central directory runs up to the end record"
    );
    assert!(count >= 1, "a package holds at least its root layer");

    let mut entries = Vec::with_capacity(count);
    let mut at = directory_offset;
    // Where the next local header must start: files are packed back to back.
    let mut expected_header = 0usize;
    for index in 0..count {
        assert_eq!(u32_at(bytes, at), CENTRAL, "central header {index}");
        let needed = u16_at(bytes, at + 6);
        assert!(needed <= 20, "no feature beyond plain zip 2.0 is needed");
        let flags = u16_at(bytes, at + 8);
        assert_eq!(flags & 0x0001, 0, "{index}: not encrypted");
        assert_eq!(flags & 0x0008, 0, "{index}: no data descriptor");
        assert_eq!(u16_at(bytes, at + 10), 0, "{index}: stored, not compressed");
        let stamp = &bytes[at + 12..at + 16];
        let crc = u32_at(bytes, at + 16);
        let compressed = u32_at(bytes, at + 20) as usize;
        let size = u32_at(bytes, at + 24) as usize;
        assert_eq!(compressed, size, "{index}: stored size equals real size");
        let name_length = u16_at(bytes, at + 28);
        let extra_length = u16_at(bytes, at + 30);
        let comment_length = u16_at(bytes, at + 32);
        assert_eq!(u16_at(bytes, at + 34), 0, "{index}: on disk 0");
        let header_offset = u32_at(bytes, at + 42) as usize;
        let name = std::str::from_utf8(&bytes[at + 46..at + 46 + name_length])
            .expect("names are UTF-8")
            .to_owned();
        assert!(!name.is_empty() && !name.starts_with('/') && !name.contains('\\'));
        assert!(
            !name.split('/').any(|part| part == ".." || part.is_empty()),
            "{name}: a plain relative path"
        );
        at += 46 + name_length + extra_length + comment_length;

        // The local header must agree with the directory.
        assert_eq!(
            header_offset, expected_header,
            "{name}: files are stored back to back in directory order"
        );
        let h = header_offset;
        assert_eq!(u32_at(bytes, h), LOCAL, "{name}: local header");
        assert_eq!(u16_at(bytes, h + 4), needed, "{name}: version needed");
        assert_eq!(u16_at(bytes, h + 6), flags, "{name}: flags");
        assert_eq!(u16_at(bytes, h + 8), 0, "{name}: stored");
        assert_eq!(&bytes[h + 10..h + 14], stamp, "{name}: time and date");
        assert_eq!(u32_at(bytes, h + 14), crc, "{name}: CRC in both headers");
        assert_eq!(
            u32_at(bytes, h + 18) as usize,
            size,
            "{name}: compressed size"
        );
        assert_eq!(u32_at(bytes, h + 22) as usize, size, "{name}: size");
        let local_name_length = u16_at(bytes, h + 26);
        let local_extra_length = u16_at(bytes, h + 28);
        assert_eq!(
            &bytes[h + 30..h + 30 + local_name_length],
            name.as_bytes(),
            "{name}: name in both headers"
        );
        let extra_start = h + 30 + local_name_length;
        let extra = bytes[extra_start..extra_start + local_extra_length].to_vec();
        // An extra field is a list of (id, size, data) records and nothing else.
        let mut e = 0;
        while e < extra.len() {
            assert!(e + 4 <= extra.len(), "{name}: an extra record has a header");
            e += 4 + u16_at(&extra, e + 2);
        }
        assert_eq!(e, extra.len(), "{name}: extra records fill the extra field");

        let data_offset = extra_start + local_extra_length;
        assert_eq!(
            data_offset % 64,
            0,
            "{name}: file data starts on a 64-byte boundary"
        );
        assert!(data_offset + size <= directory_offset, "{name}: data fits");
        let data = bytes[data_offset..data_offset + size].to_vec();
        assert_eq!(crc32(&data), crc, "{name}: CRC-32 of the data");
        expected_header = data_offset + size;
        entries.push(ZipEntry {
            name,
            data,
            header_offset,
            data_offset,
            extra,
        });
    }
    assert_eq!(at, end, "the directory holds exactly the counted entries");
    assert_eq!(
        expected_header, directory_offset,
        "the directory follows the last file directly"
    );
    let mut names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), entries.len(), "names are unique");
    entries
}
