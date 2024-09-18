use clap::Parser;
use crossterm::cursor::{DisableBlinking, MoveTo};
use crossterm::style::{
    Color::{Black, Blue, DarkYellow, White},
    Colors, SetColors,
};
use crossterm::terminal::{Clear, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};
use rand::rngs::ThreadRng;
use rand::Rng;
use rascii_art::{render_to, RenderOptions};
use std::fs::read;
use std::future::Future;
use std::io::{stdin, stdout};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, Instant};

const SECOND: Duration = Duration::from_secs(1);

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Game length in seconds.
    #[arg(short, long, default_value_t = 60)]
    game_time: u64,

    /// toggle countdown off.
    #[arg(short, long)]
    countdown: bool,

    /// make list even
    #[arg(short, long)]
    even: bool,
}

struct Delay {
    when: Instant,
    score: usize,
    word_cloud: Vec<String>,
    rng: ThreadRng,
    length: usize,
    missed_words: String,
    middle: (u16, u16),
}

impl Future for Delay {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        let mut val = String::new();
        let s = self.length;
        //will err once all words are used up
        let picker = self.rng.gen_range(0..s);
        let word = self.word_cloud.get(picker).expect("Should never reach because the rng should err first").clone();
        self.word_cloud.remove(picker);
        self.length -= 1;
        execute!(
            stdout(),
            Clear(terminal::ClearType::All),
            MoveTo(self.middle.0 - ((word.len() / 2) + 1) as u16, self.middle.1),
            //SetColors(Colors::new(White, Blue)),
        )
        .unwrap();
        let word_len_hypen: String = vec!['―'; word.len() + 2].iter().collect();
        println!("┌{word_len_hypen}┐");
        match word.len() {
            16 => {
                execute!(stdout(), MoveTo(self.middle.0 - 9, self.middle.1 + 1),).unwrap();
                println!("│ {word} │");
                execute!(stdout(), MoveTo(self.middle.0 - 9, self.middle.1 + 2),).unwrap();
                println!(
                    "│ {:02}  Seconds Left │",
                    (self.when - Instant::now()).as_secs()
                );
            }
            1..=14 => {
                let num_hyphens: String = vec!['―'; (14 - word.len()) / 2].iter().collect();
                execute!(stdout(), MoveTo(self.middle.0 - 9, self.middle.1 + 1),).unwrap();
                println!("┌{num_hyphens}┘ {word} └{num_hyphens}┐");
                execute!(stdout(), MoveTo(self.middle.0 - 9, self.middle.1 + 2),).unwrap();
                println!(
                    "│ {:02}  Seconds Left │",
                    (self.when - Instant::now()).as_secs()
                );
            }
            _ => {
                execute!(
                    stdout(),
                    MoveTo(
                        self.middle.0 - ((word.len() / 2) + 1) as u16,
                        self.middle.1 + 1
                    ),
                )
                .unwrap();
                let num_hyphens: String = vec!['―'; (word.len() - 18) / 2].iter().collect();
                println!("│ {word} │");
                execute!(
                    stdout(),
                    MoveTo(
                        self.middle.0 - ((word.len() / 2) + 2) as u16,
                        self.middle.1 + 2
                    ),
                )
                .unwrap();
                println!(
                    " └{num_hyphens}┐ {:02}  Seconds Left ┌{num_hyphens}┘",
                    (self.when - Instant::now()).as_secs()
                );
            }
        }
        execute!(stdout(), MoveTo(self.middle.0 - 9, self.middle.1 + 3),).unwrap();
        //4
        println!("└――――――――――――――――――┘");
        execute!(
            stdout(),
            MoveTo(self.middle.0, self.middle.1 + 4),
            //SetColors(Colors::new(White, Blue)),
        ).unwrap();
        stdin().read_line(&mut val).expect("fail");
        if val.starts_with('y') {
            self.score += 1;
            //thread::spawn(|| flash_color(Green));
            //flash_color(Green);
        } else if val.starts_with('q') {
            self.when = Instant::now();
        } else {
            self.missed_words.push_str(&format!("{}, ", word));
            //thread::spawn(|| flash_color(Red));
            //flash_color(Red);
        }
        if Instant::now() >= self.when {
            print_output(self.score, &mut self.missed_words);
            Poll::Ready("\n================================================================================")
        } else {
            // Ignore this line for now.
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub async fn flash_color(color: crossterm::style::Color) {
    execute!(
        stdout(),
        SetColors(Colors::new(Black, color)),
        Clear(terminal::ClearType::All),
    )
    .unwrap();
    thread::sleep(SECOND);
    execute!(
        stdout(),
        SetColors(Colors::new(White, Blue)),
        Clear(terminal::ClearType::All),
    )
    .unwrap();
}

//TODO:
//[x]  Make numbers into a square
//[n]  Up font size on questions | Not possible with terminal
//[x]  Use Bracets or hypens to emphasize questions
//[ ]  Visual Feedback of correct or incorrect. Flash screen?
//[ ]  Live timer. Tell when last guess
//[x]  Make sure no guesses are repeat
//[ ]  Make sure no guesses are repeat over multiple games
//[ ]  Repeat games after
//[ ]  Check out terminal_size for parker brown
//[ ]  Seperate by category

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if args.even {
        make_even();
        return;
    }

    let mut word_cloud: Vec<String> = Vec::new();
    String::from_utf8(read(Path::new("files/ASOIAF_list.txt")).expect("file not found!"))
        .expect("Non utf8 symbols found")
        .lines()
        .for_each(|word| word_cloud.push(word.to_string()));

    let terminal_size = term_size::dimensions().unwrap_or((64, 64));
    let time = match args.countdown {
        false => Instant::now() + Duration::from_millis(args.game_time * 1000 + 3400),
        true => Instant::now() + Duration::from_millis(args.game_time * 1000 + 400),
    };
    let future = Delay {
        when: time,
        length: word_cloud.len(),
        score: 0,
        word_cloud,
        rng: rand::thread_rng(),
        missed_words: String::new(),
        middle: (terminal_size.0 as u16 / 2, terminal_size.1 as u16 / 2),
    };

    execute!(
        stdout(),
        EnterAlternateScreen,
        terminal::SetTitle("ASOIAF Heads Up"),
        SetColors(Colors::new(White, Blue)),
        DisableBlinking,
        Clear(terminal::ClearType::All),
    )
    .unwrap();

    match args.countdown {
        false => setup(future, terminal_size).await,
        true => println!("{}", future.await),
    }
}

async fn setup(game: Delay, terminal: (usize, usize)) {
    let sqr = std::cmp::min(terminal.0, terminal.1) as u32;
    let diff = terminal.0.checked_sub(terminal.1);
    let render = RenderOptions::new()
        .width(sqr)
        .height(sqr)
        .charset(&[" ", ".", ",", "-", "*", "$", "#"]);
    for i in (1..=3).rev() {
        let mut buffer = String::new();
        render_to(format!("files/{}_skinny.png", i), &mut buffer, &render).unwrap();
        match diff {
            Some(x) => {
                let blank_space = String::from_utf8(vec![32; x / 2]).unwrap();
                buffer
                    .lines()
                    .for_each(|x| println!("{}{}{}", blank_space.clone(), x, blank_space.clone()));
            }
            None => println!("{buffer}"),
        }
        thread::sleep(SECOND);
        execute!(stdout(), Clear(terminal::ClearType::All),).unwrap();
    }
    println!("{}", game.await);
}

fn print_output(score: usize, missed_words: &mut String) {
    execute!(
        stdout(),
        LeaveAlternateScreen,
        SetColors(Colors::new(Blue, Black)),
    )
    .unwrap();
    println!("================================================================================");
    execute!(stdout(), SetColors(Colors::new(DarkYellow, Black)),).unwrap();

    println!("\nYour Score is {}!\n", score);
    execute!(stdout(), SetColors(Colors::new(Blue, Black)),).unwrap();
    println!("================================================================================");
    missed_words.pop();
    missed_words.pop();
    if !missed_words.is_empty() {
        println!("\nMissed words:\n");
        execute!(stdout(), SetColors(Colors::new(DarkYellow, Black)),).unwrap();
        println!("{}", missed_words);
    } else {
        execute!(stdout(), SetColors(Colors::new(DarkYellow, Black)),).unwrap();
        println!("\nNo Missed words!");
    }
    execute!(stdout(), SetColors(Colors::new(Blue, Black)),).unwrap();
}
fn make_even() {
    let mut word_cloud = Vec::new();
    String::from_utf8(read(Path::new("files/ASOIAF_list.txt")).expect("file not found!"))
        .expect("Non utf8 symbols found")
        .lines()
        .for_each(|word| word_cloud.push(word.to_string()));
    for word in word_cloud {
        if word.len() % 2 == 1 {
            print!(" ");
        }
        println!("{word}");
    }
}
