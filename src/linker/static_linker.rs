use crate::linker;
use crate::option;

pub struct StaticLinker {
    pub object_file: elf_utilities::file::ELF64,
    pub linker_option: option::LinkOption,
}

impl linker::Linker for StaticLinker {
    fn link(&mut self) -> elf_utilities::file::ELF64 {
        self.object_file.clone()
    }
}
