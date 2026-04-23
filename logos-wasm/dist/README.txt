This directory will contain the compiled WASM frontend assets after running:

    cd logos-wasm && trunk build --release

The logos-server binary embeds these assets at compile time using `rust-embed`.
If this directory is empty at compile time, the server will show the welcome
page instead of the frontend UI.
