extern crate cbindgen;

use std::env;

use cbindgen::Config;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::Builder::new()
        .with_config(Config::from_file("cbindgen.toml").unwrap())
        .with_crate(crate_dir)
        .with_parse_deps(true)
        .with_parse_include(&["libhi", "rosc"])
        .generate()
        .map_or_else(
            |error| match error {
                cbindgen::Error::ParseSyntaxError { .. } => {}
                e => panic!("{:?}", e),
            },
            |bindings| {
                bindings.write_to_file("../target/include/libhi.h");
            },
        );
}
