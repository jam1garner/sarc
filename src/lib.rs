mod parser;
mod writer;

#[derive(Debug)]
pub struct SarcFile {
    pub byte_order: Endian,
    pub files: Vec<SarcEntry>
}

pub struct SarcEntry {
    pub name: Option<String>,
    pub data: Vec<u8>
}

impl std::fmt::Debug for SarcEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.name)
    }
}

#[repr(u16)]
#[derive(Debug)]
pub enum Endian {
    Big = 0xFEFF,
    Little = 0xFFFE,
}

const KEY: u32 = 0x00000065;

pub fn sfat_hash(string: &str) -> u32 {
    string.chars().fold(0u32, |hash, c| hash.wrapping_mul(KEY) + (c as u32))
}

#[cfg(test)]
mod tests {
    use super::SarcFile;

    #[test]
    fn file_test() {
        let file = SarcFile::read_from_file("Animal_Fish_A.sbactorpack").unwrap();
        file.write_to_compressed_file("animal_test.sarc").unwrap();
        dbg!(file);
    }
}
