//! A simple to use library for reading/writing SARC and SZS (yaz0 compressed SARCs) in Rust.
//! 
//! ```rust
//! // yaz0 and non-yaz0 sarcs can be read the same way
//! let sarc = SarcFile::read_from_file("Animal_Fish_A.sbactorpack").unwrap();
//! 
//! // iterate through files in the sarc and print out a file list
//! for file in &sarc.files {
//!     println!("Name: {:?} | Size: {}", file.name, file.data.len());
//! }
//! 
//! // write as yaz0 compressed sarc
//! sarc.write_to_compressed_file("animal_test.sarc").unwrap();
//! ```
//!
//! ### Features
//!
//! `yaz0_sarc` - support reading/writing yaz0-compressed sarc files
//! `zstd_sarc` - support reading/writing yaz0-compressed sarc files
pub mod parser;
pub mod writer;

/// An in-memory representation of a Sarc archive
#[derive(Debug)]
pub struct SarcFile {
    pub byte_order: Endian,
    pub files: Vec<SarcEntry>
}

/// A file contained within a Sarc archive
pub struct SarcEntry {
    /// Filename of the file within the Sarc
    pub name: Option<String>,
    /// Data of the file
    pub data: Vec<u8>
}

impl std::fmt::Debug for SarcEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.name)
    }
}

/// Byte order of the give sarc file
#[repr(u16)]
#[derive(Debug)]
pub enum Endian {
    Big = 0xFEFF,
    Little = 0xFFFE,
}

const KEY: u32 = 0x00000065;

/// Hashing function used for hashing sfat strings
pub fn sfat_hash(string: &str) -> u32 {
    string.chars().fold(0u32, |hash, c| hash.wrapping_mul(KEY) + (c as u32))
}

#[cfg(test)]
mod tests {
    use super::SarcFile;

    #[test]
    fn file_test() {
        let file = SarcFile::read_from_file("Animal_Fish_A.sbactorpack").unwrap();
        #[cfg(feature = "yaz0_sarc")]
        file.write_to_compressed_file("animal_test.sarc").unwrap();
        dbg!(file);
    }

    #[test]
    fn file_test_2() {
        let file = SarcFile::read_from_file("/home/jam/a/downloads/animal_crossing/horizons/romfs/String.szs").unwrap();
        file.write_to_file("test.sarc").unwrap();
        dbg!(file);
    }
}
