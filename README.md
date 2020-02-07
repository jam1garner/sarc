# sarc

A simple to use library for reading/writing SARC and SZS (yaz0 compressed SARCs) in Rust.

```rust
// yaz0 and non-yaz0 sarcs can be read the same way
let sarc = SarcFile::read_from_file("Animal_Fish_A.sbactorpack").unwrap();

// iterate through files in the sarc and print out a file list
for file in &sarc.files {
    println!("Name: {:?} | Size: {}", file.name, file.data.len());
}

// write as yaz0 compressed sarc
sarc.write_to_compressed_file("animal_test.sarc").unwrap();
```
