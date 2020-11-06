use crate::linker;
use crate::option;
pub trait Linker {
    fn link(&mut self) -> elf_utilities::file::ELF64;
}

pub fn link_object_file(
    file_path: &str,
    link_option: option::LinkOption,
) -> Result<elf_utilities::file::ELF64Dumper, Box<dyn std::error::Error>> {
    let object_file = elf_utilities::parser::read_elf::<elf_utilities::file::ELF64>(file_path)?;

    let mut linker: Box<dyn Linker> = if link_option.static_link {
        Box::new(linker::static_linker::StaticLinker {
            object_file,
            linker_option: link_option,
        })
    } else {
        unimplemented!()
    };

    let executable_file = linker.link();

    Ok(elf_utilities::file::ELF64Dumper::new(executable_file))
}
