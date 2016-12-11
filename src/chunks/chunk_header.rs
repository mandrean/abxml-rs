use std::fmt;

pub struct ChunkHeader {
    offset: u64,
    header_size: u16,
    chunk_size: u32,
    chunk_type: u16
}

impl ChunkHeader {
    pub fn new(offset: u64, header_size: u16, chunk_size: u32, chunk_type: u16) -> Self {
        ChunkHeader {
            offset: offset,
            header_size: header_size,
            chunk_size: chunk_size,
            chunk_type: chunk_type,
        }
    }

    pub fn get_offset(&self) -> u64 {
        self.offset
    }

    pub fn get_data_offset(&self) -> u64 {
        self.offset + self.header_size as u64
    }

    pub fn get_chunk_end(&self) -> u64 {
        self.offset + self.chunk_size as u64
    }

    pub fn relative(&self, absolute: u64) -> u64 {
        if self.offset > absolute {
            0
        } else {
            absolute - self.offset
        }
    }
}

impl fmt::Display for ChunkHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "(Token:{:X}; Start: {}; Data: {}; End {})",
            self.chunk_type,
            self.offset,
            self.get_data_offset(),
            self.get_chunk_end()
        )
    }
}
