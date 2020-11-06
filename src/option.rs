pub struct LinkOption {
    /// a program's entry point.
    /// e_entry is set to this symbol's address.
    pub entry_point: String,

    pub static_link: bool,
}
