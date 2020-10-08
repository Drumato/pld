use crate::LinkOption;
use elf_utilities::{header, segment, symbol};

const PAGE_SIZE: u64 = 0x1000;
const BASE_CODE_ADDRESS: u64 = 0x400000;
const BASE_DATA_ADDRESS: u64 = 0x401000;

pub fn static_link_with(
    obj_file: elf_utilities::file::ELF64,
    link_option: LinkOption,
) -> elf_utilities::file::ELF64Dumper {
    let mut linker = StaticLinker {
        file: obj_file,
        option: link_option,
    };
    let segments = linker.initialize_segments();
    linker.file.segments = segments;

    // パディングしたのでセクションのオフセットを変更する必要がある
    // この段階で変更するのは，allocate_address_to_symbols() で,
    // このセクションオフセットを用いてセクションシンボルのst_valueを更新するため
    linker.update_sections_offset();

    // NULLセクションをアラインのため0x00で埋める．
    // これはGCCもやっている方法
    linker.adding_null_byte_to(0);

    // 実際のリンク
    let start_up_routine_address = linker.allocate_address_to_symbols();
    linker.update_entry_point(start_up_routine_address);
    linker.resolve_relocation_symbols();

    // 次に文字列データ用に，0x00 パディングを行う．
    // 二段階に分けるのは,パディングサイズを正しく計算するため．
    linker.add_null_byte_to_null_section();

    linker.update_ehdr();

    elf_utilities::file::ELF64Dumper::new(linker.file)
}

struct StaticLinker {
    file: elf_utilities::file::ELF64,
    option: LinkOption,
}

impl StaticLinker {
    fn initialize_segments(&mut self) -> Vec<elf_utilities::segment::Segment64> {
        let mut segments = Vec::new();
        let code_segment = self.init_code_segment();
        segments.push(code_segment);

        segments.extend(self.init_data_segment().iter());

        segments
    }

    fn update_ehdr(&mut self) {
        let all_section_size = self.file.all_section_size();
        let segment_number = self.file.sections.len();
        let ehdr = &mut self.file.ehdr;

        ehdr.set_elf_type(header::Type::Exec);

        ehdr.e_phoff = header::Ehdr64::size() as u64;
        ehdr.e_phnum = segment_number as u16;
        ehdr.e_phentsize = segment::Phdr64::size();

        ehdr.e_shoff = PAGE_SIZE + all_section_size;
    }

    fn add_null_byte_to_null_section(&mut self) {
        let nodata_sct = self.file.first_mut_section_by(|sct| sct.name == ".nodata");
        if nodata_sct.is_none() {
            return;
        }

        // 0x00 をセクションに書き込む
        let nodata_sct = nodata_sct.unwrap();
        let nodata_offset = nodata_sct.header.sh_offset;

        let mut extra_bytes = vec![0x00; PAGE_SIZE as usize * 2 - nodata_offset as usize];
        if nodata_sct.bytes.is_none() {
            nodata_sct.bytes = Some(Vec::new());
        }

        nodata_sct.bytes.as_mut().unwrap().append(&mut extra_bytes);
        nodata_sct.header.sh_size = PAGE_SIZE * 2 - nodata_offset;
    }

    fn adding_null_byte_to(&mut self, sct_idx: usize) {
        // 0x00を セクションに書き込む
        // section-header の値は変えないので,どのセクションにも属さないバイナリを書き込む
        let pht_size: usize = segment::Phdr64::size() as usize * self.file.segments.len();

        let mut extra_bytes =
            vec![0x00; PAGE_SIZE as usize - header::Ehdr64::size() as usize - pht_size as usize];

        let section = &mut self.file.sections[sct_idx];
        if section.bytes.is_none() {
            section.bytes = Some(Vec::new());
        }
        section.bytes.as_mut().unwrap().append(&mut extra_bytes);
    }

    fn allocate_address_to_symbols(&mut self) -> elf_utilities::Elf64Addr {
        // プロセスのエントリポイントを取得する
        // symbol.st_value には ファイルオフセットが格納されているので，
        // BASE_CODE_ADDRESS + st_value -> メモリ上のアドレス，という感じになる
        let mut ehdr_entry: elf_utilities::Elf64Addr = 0;
        let sections = self.file.sections.clone();
        let entry_point = self.option.entry_point.to_string();

        // 各シンボルにアドレスを割り当て
        if let Some(symtab_sct) = self.file.first_mut_section_by(|sct| sct.name == ".symtab") {
            let mut symbols = symtab_sct.symbols.as_ref().unwrap().clone();

            for sym in symbols.iter_mut() {
                let sym_type = sym.get_type();

                match sym_type {
                    symbol::Type::Func => {
                        // スタートアップルーチンであればエントリポイントに指定
                        if sym.compare_by(|s| s.symbol_name.as_ref().unwrap() == &entry_point) {
                            ehdr_entry = BASE_CODE_ADDRESS + sym.st_value;
                        }

                        // 相対オフセットを追加する
                        sym.st_value += BASE_CODE_ADDRESS;
                    }
                    symbol::Type::Section => {
                        // ロード先のアドレスを格納しておく
                        let related_section_index = sym.st_shndx as usize;
                        let related_section_address =
                            sections[related_section_index].header.sh_addr;

                        sym.st_value = related_section_address;
                    }
                    _ => {}
                }
            }

            symtab_sct.symbols = Some(symbols);
        }

        // update_entry_point() 用に返す
        ehdr_entry
    }

    fn resolve_relocation_symbols(&mut self) {
        let symbols = self
            .file
            .first_section_by(|sct| sct.name == ".symtab")
            .unwrap()
            .symbols
            .as_ref()
            .unwrap()
            .clone();
        let rela_symbols = self
            .file
            .first_section_by(|sct| sct.name == ".rela.text")
            .unwrap()
            .rela_symbols
            .as_ref()
            .unwrap()
            .clone();

        // 各再配置シンボルにアドレスを割り当て
        for rela_sym in rela_symbols.iter() {
            let r_info = rela_sym.get_type();

            match r_info {
                // 文字列リテラル
                elf_utilities::relocation::R_X86_64_32 => {
                    // Relaオブジェクトに対応するシンボルテーブルエントリからアドレスを取り出す
                    // rodataのオフセット + r_offsetでうまくいく
                    // セクションシンボルには allocate_address_to_symbols で予めセクションオフセットが入っている
                    let related_symbol_index = rela_sym.get_sym() as usize;
                    let rodata_offset = symbols[related_symbol_index].st_value as i32;
                    let string_offset = rodata_offset + rela_sym.get_addend() as i32;

                    // アドレスをバイト列に変換,機械語に書き込むことでアドレス解決
                    for (idx, b) in string_offset.to_le_bytes().to_vec().iter().enumerate() {
                        if let Some(text_sct) =
                            self.file.first_mut_section_by(|sct| sct.name == ".text")
                        {
                            text_sct.write_byte_to_index(*b, rela_sym.get_offset() as usize + idx);
                        }
                    }
                }
                // call
                elf_utilities::relocation::R_X86_64_PLT32 => {
                    // Relaオブジェクトに対応するシンボルテーブルエントリからアドレスを取り出す
                    let related_symbol_index = rela_sym.get_sym() as usize;
                    let sym_address = symbols[related_symbol_index].st_value as i32;
                    let relative_offset =
                        sym_address - BASE_CODE_ADDRESS as i32 - rela_sym.get_offset() as i32
                            + rela_sym.get_addend() as i32;

                    // アドレスをバイト列に変換,機械語に書き込むことでアドレス解決
                    for (idx, b) in relative_offset.to_le_bytes().to_vec().iter().enumerate() {
                        if let Some(text_sct) =
                            self.file.first_mut_section_by(|sct| sct.name == ".text")
                        {
                            text_sct.write_byte_to_index(*b, rela_sym.get_offset() as usize + idx);
                        }
                    }
                }
                _ => {
                    eprintln!("unsupported relocation type -> {}", r_info);
                }
            }
        }
    }

    fn update_sections_offset(&mut self) {
        let mut extra_bytes = 0x00;

        for (i, sct) in self.file.sections.iter_mut().enumerate() {
            let is_text_sct = sct.name == ".text";
            let is_rodata_sct = sct.name == ".rodata";

            let update_offset = if i < 6 {
                PAGE_SIZE - header::Ehdr64::size() as u64 + sct.header.sh_offset
            } else {
                // .rodataの後ろならさらにパディングされている
                let updated = PAGE_SIZE * 2 + extra_bytes;
                extra_bytes += sct.header.sh_size;

                updated
            };

            sct.header.sh_offset = update_offset;

            if is_text_sct {
                sct.header.sh_addr = BASE_CODE_ADDRESS;
            } else if is_rodata_sct {
                sct.header.sh_addr = BASE_DATA_ADDRESS;
            }
        }
    }

    fn update_entry_point(&mut self, entry: elf_utilities::Elf64Addr) {
        self.file.ehdr.e_entry = entry;
    }

    fn init_code_segment(&mut self) -> segment::Segment64 {
        let mut phdr: segment::Phdr64 = Default::default();

        // 機械語命令 -> PT_LOADに配置
        phdr.set_type(segment::Type::Load);

        // Linux環境ではページサイズアラインされている必要あり
        phdr.p_offset = PAGE_SIZE;
        phdr.p_align = PAGE_SIZE;

        // 決め打ちしたアドレスにロード
        phdr.p_vaddr = BASE_CODE_ADDRESS;
        phdr.p_paddr = BASE_CODE_ADDRESS;

        let text_section_opt = self.file.first_section_by(|sct| sct.name == ".text");

        if text_section_opt.is_none() {
            panic!("not found .text section");
        }

        let text_binary_length = text_section_opt.unwrap().header.sh_size;

        // .bssではないので filesz/memsz は同じ
        phdr.p_filesz = text_binary_length;
        phdr.p_memsz = text_binary_length;

        phdr.p_flags = segment::PF_R;

        segment::Segment64::new(phdr)
    }

    fn init_data_segment(&mut self) -> Option<segment::Segment64> {
        let mut phdr: segment::Phdr64 = Default::default();

        // 文字列データ -> PT_LOADに配置
        phdr.set_type(segment::Type::Load);

        // Linux環境ではページサイズアラインされている必要あり
        phdr.p_offset = PAGE_SIZE * 2;
        phdr.p_align = PAGE_SIZE;

        // 決め打ちしたアドレスにロード
        phdr.p_vaddr = BASE_DATA_ADDRESS;
        phdr.p_paddr = BASE_DATA_ADDRESS;

        let rodata_section_opt = self.file.first_section_by(|sct| sct.name == ".rodata");

        if rodata_section_opt.is_none() {
            return None;
        }

        let rodata_binary_length = rodata_section_opt.unwrap().header.sh_size;
        // .bssではないので， filesz/memsz は同じ
        phdr.p_filesz = rodata_binary_length;
        phdr.p_memsz = rodata_binary_length;

        phdr.p_flags = segment::PF_R;

        Some(segment::Segment64::new(phdr))
    }
}
