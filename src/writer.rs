use binwrite::{BinWrite, writer_option_new};
use super::*;
use std::io::prelude::*;
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),
    Yaz0Error(yaz0::Error)
}

impl SarcFile {
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        self.write(&mut std::fs::File::create(path.as_ref())?)
    }

    pub fn write_to_compressed_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        self.write_yaz0(
            &mut std::fs::File::create(path.as_ref())
                .map_err(|e| Error::IoError(e))?
        )
    }

    pub fn write_yaz0<W: Write>(&self, f: &mut W) -> Result<(), Error> {
        let writer = yaz0::Yaz0Writer::new(f);
        let mut temp = vec![];
        self.write(&mut temp)
            .map_err(|e| Error::IoError(e))?;
        writer.compress_and_write(&temp, yaz0::CompressionLevel::Lookahead { quality: 10 })
            .map_err(|e| Error::Yaz0Error(e))
    }

    pub fn write<W: Write>(&self, f: &mut W) -> std::io::Result<()> {
        let (string_offsets, string_section) = self.generate_string_section();
        let (data_offsets, data_section) = self.generate_data_section();

        let num_files = self.files.len();
        let data_offset = SarcHeader::SIZE + Sfat::HEADER_SIZE + (num_files * SfatEntry::SIZE)
                            + SFNT_HEADER_SIZE + string_section.len();
        let file_size = (data_offset + data_section.len()) as u32;
        let data_offset = data_offset as u32;

        let options = &match self.byte_order {
            Endian::Big => writer_option_new!(endian: binwrite::Endian::Big),
            Endian::Little => writer_option_new!(endian: binwrite::Endian::Little)
        };

        SarcHeader {
            file_size,
            data_offset
        }.write_options(f, options)?;

        Sfat {
            entries: self.get_sfat_entries(string_offsets, data_offsets)
        }.write_options(f, options)?;

        // SFNT Header
        (
            b"SFNT",
            SFNT_HEADER_SIZE as u16,
            u16::default()
        ).write_options(f, options)?;
        
        string_section.write_options(f, options)?;

        data_section.write_options(f, options)?;

        Ok(())
    }

    fn get_sfat_entries(&self, string_offsets: Vec<u32>, data_offsets: Vec<(u32, u32)>)
        -> Vec<SfatEntry<'_>>
    {
        self.files
            .iter()
            .enumerate()
            .map(|(i, file)|{
                let name: Option<&str> = file.name.as_ref().map(|a| &**a);
                SfatEntry {
                    name,
                    name_table_offset: string_offsets.get(i).map(|a| *a),
                    file_range: data_offsets[i]
                }
            })
            .collect()
    }

    fn generate_string_section(&self) -> (Vec<u32>, Vec<u8>) {
        let names: Vec<&str> =
            self.files.iter().filter_map(|a| Some(a.name.as_ref()?.as_str())).collect();

        let mut string_section = vec![];
        let offsets =
            names
                .into_iter()
                .filter_map(|string| {
                    let off = string_section.len() as u32;
                    SarcString::from(string)
                        .write(&mut string_section)
                        .ok()?;
                    Some(off)
                })
                .collect();

        (offsets, string_section)
    }

    fn generate_data_section(&self) -> (Vec<(u32, u32)>, Vec<u8>) {
        let mut data = vec![];
        (
            self.files.iter()
                .map(|file| {
                    let start = data.len() as u32;
                    file.data.write(&mut data).unwrap();
                    let end = data.len() as u32;
                    (start, end)
                })
                .collect()
            ,
            data
        )
    }
}

fn magic<B1: BinWrite + Copy, B2: BinWrite>(magic: B1) -> impl Fn(B2) -> (B1, B2) {
    move |val| (magic, val)
}

fn after<B1: BinWrite + Copy, B2: BinWrite>(after: B1) -> impl Fn(B2) -> (B2, B1) {
    move |val| (val, after)
}

#[derive(BinWrite)]
struct SarcHeader {
    #[binwrite(preprocessor(
        magic((b"SARC", Self::SIZE as u16, Self::BOM))
    ))]
    file_size: u32,
    #[binwrite(postprocessor(after(0x0100u16)), pad_after(2))]
    data_offset: u32,
}

impl SarcHeader {
    const SIZE: usize = 0x14;
    const BOM: u16 = 0xFEFF;
}

#[derive(BinWrite, Clone)]
struct SfatEntry<'a>{
    #[binwrite(preprocessor(|name: &Option<&str>| name.map(sfat_hash).unwrap_or(0)))]
    name: Option<&'a str>,
    
    #[binwrite(preprocessor(|a| {
        if let &Some(a) = a {
            (a / 4) | 0x01000000
        } else {
            0
        }
    } ))]
    name_table_offset: Option<u32>,

    file_range: (u32, u32)
}

impl<'a> SfatEntry<'a> {
    const SIZE: usize = 0x10;
}

fn sfat_header<'a>(entries: &'a Vec<SfatEntry>) -> impl BinWrite + 'a {
    (
        b"SFAT",
        Sfat::HEADER_SIZE as u16,
        entries.len() as u16,
        Sfat::HASH_KEY,
        entries
    )
}

#[derive(BinWrite)]
struct Sfat<'a> {
    #[binwrite(preprocessor(sfat_header))]
    entries: Vec<SfatEntry<'a>>
}

impl<'a> Sfat<'a> {
    const HEADER_SIZE: usize = 0xC;
    const HASH_KEY: u32 = 0x00000065;
}

#[derive(BinWrite)]
struct SarcString<'a> {
    #[binwrite(cstr, align_after(4))]
    inner: &'a str
}

impl<'b> SarcString<'b> {
    fn from(inner: &'b str) -> Self {
        Self {
            inner 
        }
    }
}

const SFNT_HEADER_SIZE: usize = 8;
