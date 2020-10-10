pub fn main() {
    let args = std::env::args().collect::<Vec<String>>();

    if args.len() != 2 {
        eprintln!("usage: ./pld <file-path>");
        std::process::exit(1);
    }
}
