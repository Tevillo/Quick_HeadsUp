# A Song of Ice and Fire Heads up game 
V0.1

## Download Rust	
If rust is not already installed, then install rust here [Rust Download](https://www.rust-lang.org/tools/install)  
If issues with windows appear consult [here](https://rust-lang.github.io/rustup/installation/windows-msvc.html)  

If you are experiencing issues with rust version then run `rustup toolchain install stable` to install the latest stable version of rust

## Build Executable
`cargo build --release && cp target/release/heads_up.exe .` will build the code and create the release exe file into your current working directory.

## Run Game
`./heads_up.exe` will run the game. To confirm a correct guess then type `y` and `<Enter>`. To pass then type `n` and `<enter>`.

## Args
game_time: CMD -  `-g <seconds>` or `--game_time <seconds>`. Set the length of the game in seconds. Default: 60 seconds 

## BUGS:

Game won't check time left until guess is passed or passed. This will allow infinite time allowed for the last guess
