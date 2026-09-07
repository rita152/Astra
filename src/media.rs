//! Image metadata shared by native views and agent adapters.

use std::{fs, path::Path};

use anyhow::{Context as _, Result};

pub(crate) fn read_image_dimensions(path: &Path) -> Result<Option<(u32, u32)>> {
    let bytes = fs::read(path).with_context(|| format!("无法读取生成的图像 {}", path.display()))?;
    Ok(encoded_image_dimensions(&bytes))
}

pub(crate) fn encoded_image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        return Some((
            u32::from_be_bytes(bytes[16..20].try_into().expect("four PNG width bytes")),
            u32::from_be_bytes(bytes[20..24].try_into().expect("four PNG height bytes")),
        ));
    }
    if bytes.starts_with(b"GIF8") && bytes.len() >= 10 {
        return Some((
            u16::from_le_bytes([bytes[6], bytes[7]]).into(),
            u16::from_le_bytes([bytes[8], bytes[9]]).into(),
        ));
    }
    if bytes.starts_with(b"\xff\xd8") {
        let mut offset = 2usize;
        while offset + 9 < bytes.len() {
            if bytes[offset] != 0xff {
                offset += 1;
                continue;
            }
            let marker = bytes[offset + 1];
            offset += 2;
            if matches!(marker, 0xd8 | 0xd9 | 0x01) || (0xd0..=0xd7).contains(&marker) {
                continue;
            }
            if offset + 2 > bytes.len() {
                break;
            }
            let segment_length =
                usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
            if segment_length < 2 || offset + segment_length > bytes.len() {
                break;
            }
            if matches!(
                marker,
                0xc0 | 0xc1
                    | 0xc2
                    | 0xc3
                    | 0xc5
                    | 0xc6
                    | 0xc7
                    | 0xc9
                    | 0xca
                    | 0xcb
                    | 0xcd
                    | 0xce
                    | 0xcf
            ) && segment_length >= 7
            {
                return Some((
                    u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]]).into(),
                    u16::from_be_bytes([bytes[offset + 3], bytes[offset + 4]]).into(),
                ));
            }
            offset += segment_length;
        }
    }
    None
}
