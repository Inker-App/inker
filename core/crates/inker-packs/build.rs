// Re-embed the tax pack whenever a file under packs/tax changes.
fn main() {
    println!("cargo:rerun-if-changed=../../../packs/tax");
}
