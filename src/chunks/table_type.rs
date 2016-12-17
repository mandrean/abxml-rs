use chunks::{Chunk, ChunkHeader};
use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt};
use std::rc::Rc;
use document::{HeaderStringTable, StringTable};
use errors::*;

pub struct TableTypeDecoder;

const MASK_COMPLEX: u16 = 0x0001;

impl TableTypeDecoder {
    pub fn decode(cursor: &mut Cursor<&[u8]>, header: &ChunkHeader)  -> Result<Chunk> {
        info!("Table type decoding @{}", header.get_offset());
        let id = cursor.read_u8()?;
        cursor.read_u8()?;  // Padding
        cursor.read_u16::<LittleEndian>()?; // Padding
        let count =  cursor.read_u32::<LittleEndian>()?;
        let start = cursor.read_u32::<LittleEndian>()?;

        info!("Resources count: {} starting @{}", count, start);

        let config = ResourceConfiguration::from_cursor(cursor)?;

        cursor.set_position(header.get_data_offset());

        let entries = Self::decode_entries(cursor, count).chain_err(|| "Entry decoding failed")?;

        Ok(Chunk::TableType(id, Box::new(config), entries))
    }

    fn decode_entries(cursor: &mut Cursor<&[u8]>, entry_amount: u32) -> Result<Vec<Entry>> {
        let base_offset = cursor.position();
        let mut entries = Vec::new();
        let mut offsets = Vec::new();

        for i in 0..entry_amount {
            debug!("Entry {}/{}", i, entry_amount - 1);
            let offset = cursor.read_u32::<LittleEndian>()?;
            offsets.push(offset);
            let prev_pos = cursor.position();

            if offset == 0xFFFFFFFF {
                continue;
            }

            let maybe_entry = Self::decode_entry(cursor, base_offset, offset as u64)?;

            match maybe_entry {
                Some(e) => entries.push(e),
                None => {
                    debug!("Entry with a negative count");
                }
            }

            cursor.set_position(prev_pos);
        }

        Ok(entries)
    }

    fn decode_entry(cursor: &mut Cursor<&[u8]>, base_offset: u64, offset: u64) -> Result<Option<Entry>> {
        let position = cursor.position();
        cursor.set_position(base_offset + offset as u64);

        let header_size = cursor.read_u16::<LittleEndian>()?;
        let flags = cursor.read_u16::<LittleEndian>()?;
        let key_index = cursor.read_u32::<LittleEndian>()?;

        let header_entry = EntryHeader::new(header_size, flags, key_index);

        if header_entry.is_complex() {
            Self::decode_complex_entry(cursor, &header_entry)
        } else {
            Self::decode_simple_entry(cursor, &header_entry)
        }
    }

    fn decode_simple_entry(cursor: &mut Cursor<&[u8]>, header: &EntryHeader) -> Result<Option<Entry>> {
        let size = cursor.read_u16::<LittleEndian>()?;
        // Padding
        cursor.read_u8()?;
        let val_type = cursor.read_u8()?;
        let data = cursor.read_u32::<LittleEndian>()?;

        let entry = Entry::new_simple(
            header.get_key_index(),
            size,
            val_type,
            data,
        );

        Ok(Some(entry))
    }

    fn decode_complex_entry(cursor: &mut Cursor<&[u8]>, header: &EntryHeader) -> Result<Option<Entry>> {
        let parent_entry = cursor.read_u32::<LittleEndian>()?;
        let value_count = cursor.read_u32::<LittleEndian>()?;
        let mut entries = Vec::with_capacity(value_count as usize);

        if value_count == 0xFFFFFFFF {
            return Ok(None);
        }

        for j in 0..value_count {
            debug!("Parsing value: {}/{} (@{})", j, value_count - 1, cursor.position());
            // println!("Parsing value #{}", j);
            let val_id = cursor.read_u32::<LittleEndian>()?;
            // Resource value
            let size = cursor.read_u16::<LittleEndian>()?;
            // Padding
            cursor.read_u8()?;
            let val_type = cursor.read_u8()?;
            let data = cursor.read_u32::<LittleEndian>()?;

            let simple_entry = Entry::new_simple(
                header.get_key_index(),
                size,
                val_type,
                data,
            );

            entries.push(simple_entry);
        }

        let entry = Entry::new_complex(header.get_key_index(), parent_entry, entries);

        Ok(Some(entry))
    }
}

pub struct EntryHeader {
    header_size: u16,
    flags: u16,
    key_index: u32,
}

impl EntryHeader {
    pub fn new(header_size: u16, flags: u16, key_index: u32) -> Self {
        EntryHeader {
            header_size: header_size,
            flags: flags,
            key_index: key_index,
        }
    }

    pub fn is_complex(&self) -> bool {
        (self.flags & MASK_COMPLEX) > 0
    }

    pub fn get_key_index(&self) -> u32 {
        self.key_index
    }
}

#[derive(Debug)]
pub enum Entry {
    Simple {
        key_index: u32,
        size: u16,
        value_type: u8,
        value_data: u32,
    },
    Complex {
        key_index: u32,
        parent_entry_id: u32,
        entries: Vec<Entry>,   // TODO: split this class, Entry will be Entry::Simple here and it can be enforce by type system
    }
}

impl Entry {
    pub fn new_simple(
        key_index: u32,
        size: u16,
        value_type: u8,
        value_data: u32,
    ) -> Self {
        Entry::Simple {
            key_index: key_index,
            size: size,
            value_type: value_type,
            value_data: value_data,
        }
    }

    pub fn new_complex(
        key_index: u32,
        parent_entry_id: u32,
        entries: Vec<Entry>,
    ) -> Self {
        Entry::Complex{
            key_index: key_index,
            parent_entry_id: parent_entry_id,
            entries: entries,
        }
    }
}

pub struct Region {
    low: u8,
    high: u8,
}

impl Region {
    pub fn new(low: u8, high: u8) -> Self {
        Region {
            low: low,
            high: high,
        }
    }

    pub fn to_string(&self) -> Result<String> {
        let mut chrs = Vec::new();

        if ((self.low >> 7) & 1) == 1 {
            chrs.push(self.high & 0x1F);
            chrs.push(((self.high & 0xE0) >> 5 ) + ((self.low & 0x03) << 3));
            chrs.push((self.low & 0x7C) >> 2);
        } else {
            chrs.push(self.low);
            chrs.push(self.high);
        }

        String::from_utf8(chrs).chain_err(|| "Could not UTF-8 encode string")
    }
}

#[derive(Debug)]
pub struct ResourceConfiguration {
    size: u32,
    mcc: u16,
    mnc: u16,
    language: String,
    region: String,
    orientation : u8,
    touchscreen: u8,
    density: u16,
    keyboard: u8,
    navigation: u8,
    input_flags: u8,
    width: u16,
    height: u16,
    sdk_version: u16,
    min_sdk_version: u16,
    screen_layout: u8,
    ui_mode: u8,
    smallest_screen: u16,
    screen_width_dp: u16,
    screen_height_dp: u16,
    locale_script: Option<String>,
    locale_variant: Option<String>,
    secondary_screen_layout: Option<u8>,
}

impl ResourceConfiguration {
    pub fn from_cursor(mut cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        let initial_position = cursor.position();
        let size = cursor.read_u32::<LittleEndian>()?;
        let mcc = cursor.read_u16::<LittleEndian>()?;
        let mnc = cursor.read_u16::<LittleEndian>()?;

        let lang1 = cursor.read_u8()?;
        let lang2 = cursor.read_u8()?;

        let lang = Region::new(lang1, lang2);
        let str_lang = lang.to_string()?;

        let reg1 = cursor.read_u8()?;
        let reg2 = cursor.read_u8()?;

        let reg = Region::new(reg1, reg2);
        let str_reg = reg.to_string()?;

        let orientation = cursor.read_u8()?;
        let touchscreen = cursor.read_u8()?;

        let density = cursor.read_u16::<LittleEndian>()?;

        let keyboard = cursor.read_u8()?;
        let navigation = cursor.read_u8()?;
        let input_flags = cursor.read_u8()?;

        cursor.read_u8()?; // Padding

        let width = cursor.read_u16::<LittleEndian>()?;
        let height = cursor.read_u16::<LittleEndian>()?;
        let sdk_version = cursor.read_u16::<LittleEndian>()?;
        let min_sdk_version = cursor.read_u16::<LittleEndian>()?;

        let mut screen_layout = 0;
        let mut ui_mode = 0;
        let mut smallest_screen = 0;
        let mut screen_width_dp = 0;
        let mut screen_height_dp = 0;

        if size >= 32 {
            screen_layout = cursor.read_u8()?;
            ui_mode = cursor.read_u8()?;
            smallest_screen = cursor.read_u16::<LittleEndian>()?;
        }

        if size >= 36 {
            screen_width_dp = cursor.read_u16::<LittleEndian>()?;
            screen_height_dp = cursor.read_u16::<LittleEndian>()?;
        }

        if size >= 48 {
            // TODO: Read following bytes
            cursor.read_u32::<LittleEndian>()?;
            cursor.read_u32::<LittleEndian>()?;
            cursor.read_u32::<LittleEndian>()?;
        }

        if size >= 52 {
            // TODO: Read bytes
        }

        let rc = ResourceConfiguration {
            size: size,
            mcc: mcc,
            mnc: mnc,
            language: str_lang,
            region: str_reg,
            orientation: orientation,
            touchscreen: touchscreen,
            density: density,
            keyboard: keyboard,
            navigation: navigation,
            input_flags: input_flags,
            width: width,
            height: height,
            sdk_version: sdk_version,
            min_sdk_version: min_sdk_version,
            screen_layout: screen_layout,
            ui_mode: ui_mode,
            smallest_screen: smallest_screen,
            screen_width_dp: screen_width_dp,
            screen_height_dp: screen_height_dp,
            locale_script: None,
            locale_variant: None,
            secondary_screen_layout: None,
        };

        Ok(rc)
    }
}
