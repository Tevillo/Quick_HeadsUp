A Song of Ice and Fire Heads up game

V0.1

install rust here [Rust Download](https://www.rust-lang.org/tools/install)

Passage taken from link

"You may need to install the Visual Studio C++ Build tools when prompted to do so. If you are not on Windows see "Other Installation Methods"."

If prompted with an issue involving Visual Studio C++ then install accordingly



Then run `rustup toolchain install stable`

`cargo build` will compile the program. Then run `cargo run` and it will start the program. The game works by generating 
a random prompt and then the user will say if they got it right or wrong by typing "y" for yes and anything else for pass.

BUGS:

Game won't check time left until guess is passed or passed.
