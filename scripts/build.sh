mkdir -p res
RUSTFLAGS='-C link-arg=-s' cargo +stable build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/vault_token.wasm ./res/
