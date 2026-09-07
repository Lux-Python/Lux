use super::super::error::CacheError;

const EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const CENTRAL_DIR_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const LOCAL_HEADER_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// End of Central Directory (EOCD) record parsed from the tail of a zip archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EocdRecord {
    /// Total number of entries in the central directory.
    pub total_entries: u16,
    /// Size of the central directory in bytes.
    pub cd_size: u32,
    /// Byte offset of the start of the central directory relative to the archive start.
    pub cd_offset: u32,
}

impl EocdRecord {
    /// Search backwards from the end of a buffer (typically the last 64 KB of a wheel)
    /// to locate and parse the End of Central Directory (EOCD) record.
    pub fn parse(buffer: &[u8]) -> Result<Self, CacheError> {
        if buffer.len() < 22 {
            return Err(CacheError::InvalidZip {
                reason: "archive tail too short for EOCD record".to_string(),
            });
        }

        // Scan backwards for the 4-byte signature [0x50, 0x4b, 0x05, 0x06]
        let max_search = buffer.len().saturating_sub(22);
        let min_search = buffer.len().saturating_sub(65536 + 22);

        for i in (min_search..=max_search).rev() {
            if buffer[i..i + 4] == EOCD_SIGNATURE {
                let total_entries = u16::from_le_bytes([buffer[i + 10], buffer[i + 11]]);
                let cd_size = u32::from_le_bytes([
                    buffer[i + 12],
                    buffer[i + 13],
                    buffer[i + 14],
                    buffer[i + 15],
                ]);
                let cd_offset = u32::from_le_bytes([
                    buffer[i + 16],
                    buffer[i + 17],
                    buffer[i + 18],
                    buffer[i + 19],
                ]);

                return Ok(Self {
                    total_entries,
                    cd_size,
                    cd_offset,
                });
            }
        }

        Err(CacheError::InvalidZip {
            reason: "no EOCD signature found in archive tail".to_string(),
        })
    }
}

/// Metadata entry parsed from the Central Directory of a zip archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntryHeader {
    /// Path and filename inside the archive.
    pub filename: String,
    /// Compression algorithm (0 = Stored / Uncompressed, 8 = Deflated).
    pub compression_method: u16,
    /// Compressed size in bytes.
    pub compressed_size: u64,
    /// Uncompressed size in bytes.
    pub uncompressed_size: u64,
    /// Byte offset of the local file header relative to the archive start.
    pub local_header_offset: u64,
}

impl ZipEntryHeader {
    /// Whether this entry represents a directory rather than a file.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.filename.ends_with('/')
    }

    /// Whether this entry represents the package dist-info METADATA file.
    #[must_use]
    pub fn is_metadata(&self) -> bool {
        let lower = self.filename.to_ascii_lowercase();
        lower.ends_with(".dist-info/metadata")
    }
}

/// Parse all central directory headers from a slice containing the central directory bytes.
pub fn parse_central_directory(buffer: &[u8]) -> Result<Vec<ZipEntryHeader>, CacheError> {
    let mut entries = Vec::new();
    let mut offset = 0;

    while offset + 46 <= buffer.len() {
        if buffer[offset..offset + 4] != CENTRAL_DIR_SIGNATURE {
            break;
        }

        let compression_method = u16::from_le_bytes([buffer[offset + 10], buffer[offset + 11]]);
        let compressed_size = u64::from(u32::from_le_bytes([
            buffer[offset + 20],
            buffer[offset + 21],
            buffer[offset + 22],
            buffer[offset + 23],
        ]));
        let uncompressed_size = u64::from(u32::from_le_bytes([
            buffer[offset + 24],
            buffer[offset + 25],
            buffer[offset + 26],
            buffer[offset + 27],
        ]));
        let filename_len = usize::from(u16::from_le_bytes([
            buffer[offset + 28],
            buffer[offset + 29],
        ]));
        let extra_len = usize::from(u16::from_le_bytes([
            buffer[offset + 30],
            buffer[offset + 31],
        ]));
        let comment_len = usize::from(u16::from_le_bytes([
            buffer[offset + 32],
            buffer[offset + 33],
        ]));
        let local_header_offset = u64::from(u32::from_le_bytes([
            buffer[offset + 42],
            buffer[offset + 43],
            buffer[offset + 44],
            buffer[offset + 45],
        ]));

        offset += 46;
        if offset + filename_len > buffer.len() {
            return Err(CacheError::InvalidZip {
                reason: "truncated filename in central directory".to_string(),
            });
        }

        let filename_bytes = &buffer[offset..offset + filename_len];
        let filename = String::from_utf8_lossy(filename_bytes).to_string();

        offset += filename_len + extra_len + comment_len;

        entries.push(ZipEntryHeader {
            filename,
            compression_method,
            compressed_size,
            uncompressed_size,
            local_header_offset,
        });
    }

    Ok(entries)
}

/// Compute the actual byte offset where file payload begins by reading the local file header.
pub fn parse_local_header_data_offset(local_header: &[u8]) -> Result<u64, CacheError> {
    if local_header.len() < 30 {
        return Err(CacheError::InvalidZip {
            reason: "local header buffer too small".to_string(),
        });
    }

    if local_header[0..4] != LOCAL_HEADER_SIGNATURE {
        return Err(CacheError::InvalidZip {
            reason: "invalid local file header signature".to_string(),
        });
    }

    let filename_len = u64::from(u16::from_le_bytes([local_header[26], local_header[27]]));
    let extra_len = u64::from(u16::from_le_bytes([local_header[28], local_header[29]]));

    Ok(30 + filename_len + extra_len)
}

/// Decompress entry bytes according to its compression method.
pub fn decompress_entry(data: &[u8], compression_method: u16) -> Result<Vec<u8>, CacheError> {
    match compression_method {
        0 => Ok(data.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec(data).map_err(|e| CacheError::InvalidZip {
            reason: format!("deflate decompression error: {e:?}"),
        }),
        method => Err(CacheError::InvalidZip {
            reason: format!("unsupported zip compression method {method}"),
        }),
    }
}