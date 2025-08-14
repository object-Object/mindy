fn main() {
    #[cfg(feature = "std")]
    {
        println!("cargo:rerun-if-changed=src/logic/grammar.lalrpop");
        lalrpop::process_root().unwrap();
    }

    #[cfg(feature = "embedded_graphics")]
    {
        use std::{env, fs, path::PathBuf};

        use eg_font_converter::FontConverter;

        let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

        let fonts_dir = out_dir.join("fonts");
        fs::create_dir(&fonts_dir).ok();

        println!("cargo:rerun-if-changed=fonts/mindustry/logic.bdf");

        // https://github.com/Anuken/Mindustry/blob/65a50a97423431640e636463dde97f6f88a2b0c8/core/src/mindustry/ui/Fonts.java#L88C27-L88C126
        FontConverter::with_file("fonts/mindustry/logic.bdf", "LOGIC")
        .glyphs("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz1234567890\"!`?'.,;:()[]{}<>|/@\\^$€-%+=#_&~* ")
        .replacement_character(' ')
        .convert_mono_font()
        .unwrap()
        .save(&fonts_dir)
        .unwrap();
    }
}
