# mindustry-rs

Rust tools for Mindustry.

## mlogv32

### Profiling

[samply](https://github.com/mstange/samply):

```sh
cargo build --profile profiling --features mlogv32 --bin mlogv32
samply record target/profiling/mlogv32.exe schematics/mlogv32.msch --bin path/to/coremark.bin --delta 6
```
