fn main() {
    #[cfg(feature = "std")]
    {
        println!("cargo:rerun-if-changed=src/logic/grammar.lalrpop");
        lalrpop::process_root().unwrap();
    }
}
