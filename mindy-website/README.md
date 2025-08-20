# mindy-website

## Running

Install `wasm-pack`: https://drager.github.io/wasm-pack/installer/

```sh
rustup target add wasm32-unknown-unknown
wasm-pack build --target web --dev

cd www
yarn
yarn dev
```
