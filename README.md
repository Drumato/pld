[![x64_static_linker at crates.io](https://img.shields.io/crates/v/x64-static-linker.svg)](https://crates.io/crates/x64-static-linker)  [![x64_static_linker at docs.rs](https://docs.rs/x64-static_linker/badge.svg)](https://docs.rs/x64-static-linker)

# x64_static_linker
x86_64 static linker

## Get Started

### How to use as an linker command

```
cargo build
./target/debug/x64_static_linker <object-file>
```

### How to use as a Rust crate

See **[documentation](https://docs.rs/x64_static_linker)**

```rust
use x64_static_linker;

fn main() -> Result<(), Box<dyn std::error::Error>>{
    // you can pass a file(or string).
    let elf_builder = x64_static_linker::static_link_with("obj.o")?;
    
    elf_builder.generate_elf_file(0o644);

    Ok(())
}
```

##  Dependencies

- [Drumato/elf-utilities](https://github.com/Drumato/elf-utilities)
