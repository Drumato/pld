pub fn main() {
    let args = std::env::args().collect::<Vec<String>>();

    if args.len() != 2 {
        eprintln!("usage: ./x64_static_linker <file-path>");
        std::process::exit(1);
    }
}