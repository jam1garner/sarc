use nom::{
    IResult,
    bytes::complete::tag,
    sequence::tuple,
    multi::count,
    number::complete::*
};
use super::{SarcFile, SarcEntry, Endian};
use std::ops::Range;

impl From<u16> for Endian {
    fn from(val: u16) -> Self {
        match val {
            0xFEFF => Self::Big,
            0xFFFE => Self::Little,
            _ => panic!()
        }
    }
}

#[allow(dead_code)]
struct SarcHeader {
    byte_order: Endian,
    file_size: u32,
    data_offset: u32,
}

struct SfatNode {
    name_offset: Option<u16>,
    file_range: Range<usize>,
}


fn parse_sfat<E: TakeEndian>(data: &[u8]) -> IResult<&[u8], (u32, Vec<SfatNode>)> {
    let (data, (
        _,
        _,
        node_count,
        hash_key
    )) = tuple((
        tag(b"SFAT"),
        take_u16::<E>,
        take_u16::<E>,
        take_u32::<E>
    ))(data)?;

    let (data, files) = count::<_, _, NE, _>(|data| {
        let (data, (
            _,
            file_attrs,
            file_start,
            file_end
        )) = tuple((
            take_u32::<E>,
            take_u32::<E>,
            take_u32::<E>,
            take_u32::<E>,
        ))(data)?;

        const HAS_NAME: u32 = 0x01000000;

        let name_offset = if file_attrs & HAS_NAME != 0 {
            Some(file_attrs as u16)
        } else {
            None
        };

        Ok((data, SfatNode{
            name_offset,
            file_range: (file_start as usize..file_end as usize)
        }))
    }, node_count as _)(data)?;
    
    Ok((data, (hash_key, files)))
}

fn get_string(slice: &[u8], offset: usize) -> Option<String> {
    for i in offset..slice.len() {
        if slice[i] == 0 {
            return std::str::from_utf8(&slice[offset..i]).ok().map(String::from)
        }
    }
    None
}

type NE<'a> = (&'a [u8], nom::error::ErrorKind);

/// An error while reading the file
#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),

    ParseError(String),

    #[cfg(feature = "yaz0_sarc")]
    Yaz0Error(yaz0::Error),
}

use std::io::Cursor;
#[cfg(feature = "yaz0_sarc")]
use yaz0::Yaz0Archive;

impl SarcFile {
    /// Read a sarc file (with or without compression) from a file.
    ///
    /// **Note:** Compression requires the `yaz0_sarc` and/or the `zstd_sarc` features.
    pub fn read_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Error> {
        Self::read(&std::fs::read(path.as_ref()).map_err(|e| Error::IoError(e))?)
    }

    /// Read a sarc file (with or without compression) from a byte slice.
    ///
    /// **Note:** Compression requires the `yaz0_sarc` and/or the `zstd_sarc` features.
    pub fn read(data: &[u8]) -> Result<Self, Error> {
        let mut decompressed: Vec<u8>;
        if data.len() < 4 {
            return Err(Error::ParseError("Input buffer must be at least 4 bytes".into()));
        }
        let data = {
            if b"Yaz0" == &data[..4] {
                #[cfg(feature = "yaz0_sarc")] {
                    let mut yaz0_reader = Yaz0Archive::new(Cursor::new(data)).map_err(|e| Error::Yaz0Error(e))?;
                    decompressed = yaz0_reader.decompress().map_err(|e| Error::Yaz0Error(e))?;
                    &decompressed
                }
                #[cfg(not(feature = "yaz0_sarc"))] {
                    return Err(Error::ParseError(
                        "Yaz0 compression detected but yaz0_sarc feature not enabled.".into()
                    ));
                } 
            } else if b"\x28\xB5\x2F\xFD" == &data[..4] {  
                #[cfg(feature = "zstd_sarc")] {
                    decompressed = vec![];
                    zstd::stream::copy_decode(
                        std::io::Cursor::new(data),
                        &mut decompressed
                    ).map_err(|e| Error::IoError(e))?;
                    &decompressed
                }
                #[cfg(not(feature = "zstd_sarc"))] {
                    return Err(Error::ParseError(
                        "ZSTD compression detected but zstd_sarc feature not enabled.".into()
                    ));
                } 
            } else {
                data
            }
        };
        Self::parse(data)
            .map(|a| a.1)
            .map_err(|err| Error::ParseError(err.to_string()))
    }

    fn parse(data: &[u8]) -> IResult<&[u8], Self> {
        let (after_header, SarcHeader {
            byte_order,
            file_size: _,
            data_offset
        }) = SarcHeader::parse(data)?;

        let file_data = &data[data_offset as usize..];

        let (data, (_, files)) = match byte_order {
            Endian::Big => parse_sfat::<BigEndian>(after_header)?,
            Endian::Little => parse_sfat::<LittleEndian>(after_header)?
        };

        let string_data = &data[0x8..];

        let files: Vec<_> =
            files.into_iter()
                .map(|SfatNode { name_offset, file_range }| {
                    let name = name_offset.map(
                        |off| get_string(string_data, (off as usize) * 4)
                    ).flatten();
                    let data = Vec::from(&file_data[file_range]);

                    SarcEntry { name, data }
                })
                .collect();


        Ok((data, SarcFile {
            byte_order,
            files
        }))
    }
}

impl SarcHeader {
    fn parse(data: &[u8]) -> IResult<&[u8], Self> {
        let (data, (
            _,
            _,
            endian,
        )) = tuple::<_, _, NE, _>((
            tag(b"SARC"),
            le_u16,
            be_u16,
        ))(data)?;

        match endian.into() {
            Endian::Big => Self::parse_endian::<BigEndian>(data, Endian::Big),
            Endian::Little => Self::parse_endian::<LittleEndian>(data, Endian::Little)
        }
    }

    fn parse_endian<E: TakeEndian>(data: &[u8], byte_order: Endian) -> IResult<&[u8], Self> {
        let (data, (
            file_size,
            data_offset,
            _
        )) = tuple((
            take_u32::<E>,
            take_u32::<E>,
            take_u32::<E>
        ))(data)?;

        Ok((data, Self {
            byte_order,
            file_size,
            data_offset
        }))
    }
}

trait TakeEndian {
    fn take_u32(a: &[u8]) -> IResult<&[u8], u32>;
    fn take_u16(a: &[u8]) -> IResult<&[u8], u16>;
}

struct LittleEndian;
struct BigEndian;

impl TakeEndian for LittleEndian {
    fn take_u32(data: &[u8]) -> IResult<&[u8], u32> {
        le_u32(data)
    }
    
    fn take_u16(data: &[u8]) -> IResult<&[u8], u16> {
        le_u16(data)
    }
}

impl TakeEndian for BigEndian {
    fn take_u32(data: &[u8]) -> IResult<&[u8], u32> {
        be_u32(data)
    }

    fn take_u16(data: &[u8]) -> IResult<&[u8], u16> {
        be_u16(data)
    }
}

fn take_u32<Endian: TakeEndian>(data: &[u8]) -> IResult<&[u8], u32> {
    Endian::take_u32(data)
}

fn take_u16<Endian: TakeEndian>(data: &[u8]) -> IResult<&[u8], u16> {
    Endian::take_u16(data)
}
