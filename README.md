# Mindy

A Rust library for emulating Mlog (the logic system from [Mindustry](https://mindustrygame.github.io/)).

## mlogv32

### Profiling

[samply](https://github.com/mstange/samply):

```sh
cargo build --profile profiling --features mlogv32 --bin mlogv32
samply record target/profiling/mlogv32.exe schematics/mlogv32.msch --bin path/to/coremark.bin --delta 6
```

[Iai-Callgrind](https://iai-callgrind.github.io/iai-callgrind):

```sh
cargo bench --features mlogv32 --bench coremark -- --nocapture
```

## Why this name?

Just add Rust!

## Attribution

This crate reimplements code from [Mindustry](https://github.com/Anuken/Mindustry) and [Arc](https://github.com/Anuken/Arc) by [Anuken](https://github.com/Anuken). Mindustry is an excellent game, please go buy and play it :)

The data in `mindy::types::content` is generated from [mimex-data](https://github.com/cardillan/mimex-data) by [Cardillan](https://github.com/cardillan).
