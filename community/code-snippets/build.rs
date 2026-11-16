use std::{env, path::PathBuf};
use syntect::{dumps::dump_to_uncompressed_file, parsing::SyntaxSetBuilder};
fn main() {
    println!("cargo:rerun-if-changed=syntaxes");
    let mut builder = SyntaxSetBuilder::new();
    builder
        .add_from_folder("syntaxes", true)
        .expect("plugin lexical grammars");
    dump_to_uncompressed_file(
        &builder.build(),
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("syntaxes.bin"),
    )
    .unwrap();
}
