use binwrite::{BinWrite, writer_option_new};
use super::*;
use std::io::prelude::*;
use std::io::BufWriter;
use std::path::Path;
use std::collections::HashMap;

/// An error raised in the process of writing the sarc file
#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),

    #[cfg(feature = "yaz0_sarc")]
    Yaz0Error(yaz0::Error),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl SarcFile {
    /// Write 
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        self.write(&mut BufWriter::new(std::fs::File::create(path.as_ref())?))
    }

    /// Write to a compressed file. This writes the SARC with yaz0 compression. Requires either the
    /// `yaz0_sarc` feature or `zstd_sarc` feature enabled.
    ///
    /// **Note:** If yaz0 compression is disabled and zstd compression is enabled, this will write
    /// with zstd compression.
    #[cfg(feature = "yaz0_sarc")]
    pub fn write_to_compressed_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        self.write_yaz0(
            &mut BufWriter::new(std::fs::File::create(path.as_ref())?)
        )
    }

    /// Write to a compressed file. This writes the SARC with yaz0 compression. Requires either the
    /// `yaz0_sarc` feature or `zstd_sarc` feature enabled.
    ///
    /// **Note:** If yaz0 compression is disabled and zstd compression is enabled, this will write
    /// with zstd compression.
    #[cfg(feature = "zstd_sarc")]
    #[cfg(not(feature = "yaz0_sarc"))]
    pub fn write_to_compressed_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        self.write_zstd(
            &mut BufWriter::new(std::fs::File::create(path.as_ref())?)
        )
    }

    /// Write to a compressed file. This writes the SARC with yaz0 compression. Requires `yaz0_sarc` feature
    #[cfg(feature = "yaz0_sarc")]
    pub fn write_to_yaz0_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        self.write_yaz0(
            &mut std::fs::File::create(path.as_ref())
                .map(BufWriter::new)
                .map_err(|e| Error::IoError(e))?
        )
    }

    /// Write to a writer that implements [`std::io::Write`](std::io::Write). This writes the SARC with yaz0 
    /// compression. Requires `yaz0_sarc` feature.
    #[cfg(feature = "yaz0_sarc")]
    pub fn write_yaz0<W: Write>(&self, f: &mut W) -> Result<(), Error> {
        let writer = yaz0::Yaz0Writer::new(f);
        let mut temp = vec![];
        self.write(&mut temp)?;
        writer.compress_and_write(&temp, yaz0::CompressionLevel::Lookahead { quality: 10 })
            .map_err(|e| Error::Yaz0Error(e))
    }

    /// Write to a writer that implements [`std::io::Write`](std::io::Write). This writes the SARC with zstd
    /// compression. Requires `zstd_sarc` feature.
    #[cfg(feature = "zstd_sarc")]
    pub fn write_zstd<W: Write>(&self, f: &mut W) -> Result<(), Error> {
        let mut writer =
            zstd::stream::Encoder::new(f, zstd::DEFAULT_COMPRESSION_LEVEL)?;
        self.write(&mut writer)?;
        writer.finish().unwrap();
        Ok(())
    }

    /// Write to a writer that implements [`std::io::Write`](std::io::Write). This writes the SARC with no 
    /// compression.
    pub fn write<W: Write>(&self, f: &mut W) -> std::io::Result<()> {
        let (string_offsets, string_section) = self.generate_string_section();
        let (data_offsets, data_section) = self.generate_data_section();

        let num_files = self.files.len();
        let data_padding_offset = SarcHeader::SIZE + Sfat::HEADER_SIZE
            + (num_files * SfatEntry::SIZE) + SFNT_HEADER_SIZE + string_section.len();
        let data_offset = (data_padding_offset + 0x1FFF) & !0x1FFF;
        let data_padding = data_offset - data_padding_offset;

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

        vec![0u8; data_padding].write_options(f, options)?;

        data_section.write_options(f, options)?;

        f.flush()
    }

    fn get_sfat_entries(&self, string_offsets: HashMap<u32, u32>, data_offsets: HashMap<u32, (u32, u32)>)
        -> Vec<SfatEntry<'_>>
    {
        let mut sfat_entries: Vec<SfatEntry<'_>> = self.files
            .iter()
            .map(|file| {
                let name: Option<&str> = file.name.as_deref();
                SfatEntry {
                    name,
                    name_table_offset:
                        name.map(sfat_hash)
                            .and_then(|hash| string_offsets.get(&hash).copied()),
                    file_range: data_offsets[&name.as_deref().map(sfat_hash).unwrap_or_default()]
                }
            })
            .collect();
        sfat_entries.sort_by_key(|e| e.name.map(sfat_hash).unwrap_or(0));
        sfat_entries
    }

    fn generate_string_section(&self) -> (HashMap<u32, u32>, Vec<u8>) {
        let mut names: Vec<&str> =
            self.files.iter().filter_map(|a| Some(a.name.as_ref()?.as_str())).collect();

        let mut string_section = vec![];
        names.sort_by_key(|name| sfat_hash(name));
        let offsets =
            names
                .into_iter()
                .filter_map(|string| {
                    let off = string_section.len() as u32;
                    SarcString::from(string)
                        .write(&mut string_section)
                        .ok()?;
                    Some((sfat_hash(string), off))
                })
                .collect();

        (offsets, string_section)
    }

    fn generate_data_section(&self) -> (HashMap<u32, (u32, u32)>, Vec<u8>) {
        let mut data = vec![];
        let mut files: Vec<_> = self.files.iter()
            .map(|file| (file.name.as_deref().map(sfat_hash).unwrap_or_default(), &file.data[..]))
            .collect();
        files.sort_by_key(|(hash, _)| *hash);
        (
            files.into_iter()
                .map(|(hash, file)| {
                    let start_padding = data.len();
                    let start = (start_padding + 0x1fff) & !0x1fff;
                    let padding = start - start_padding;
                    let start = start as u32;
                    vec![0u8; padding].write(&mut data).unwrap();
                    file.write(&mut data).unwrap();
                    let end = data.len() as u32;
                    (hash, (start, end))
                })
                .collect(),
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
