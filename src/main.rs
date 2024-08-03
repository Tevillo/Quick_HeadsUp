use crossterm::cursor::{MoveToPreviousLine, MoveTo, DisableBlinking};
use crossterm::{execute, terminal};
use crossterm::terminal::{window_size, Clear, EnterAlternateScreen, LeaveAlternateScreen, WindowSize};
use rand::rngs::ThreadRng;
use rand::Rng;
use std::fs::read;
use std::thread;
use std::future::Future;
use std::io::{stdin, stdout};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use crossterm::style::{Color::{Blue,White,Black,DarkYellow}, Colors, SetColors};
use clap::Parser;
use rascii_art::{render_to,RenderOptions,};

const SECOND: Duration = Duration::from_secs(1);

#[derive(Parser,Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Game length in seconds.
    #[arg(short, long, default_value_t = 60)]
    game_time: u64,
}

struct Delay {
    when: Instant,
    score: usize,
    word_cloud: Vec<String>,
    rng: ThreadRng,
    length: usize,
    missed_words: String,
    middle: (u16,u16),
}

impl Future for Delay {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        let mut val = String::new();
        let s = self.length;
        let picker = self.rng.gen_range(0..s);
        let word = self.word_cloud.get(picker).unwrap().clone();
        self.word_cloud.remove(picker);
        self.length -= 1;
        execute!(
            stdout(),
            MoveTo(self.middle.0 - (word.len() / 2) as u16,self.middle.1),
            Clear(terminal::ClearType::FromCursorDown),
        ).unwrap();
        println!("{word}");
        execute!(
            stdout(),
            MoveTo(self.middle.0 - 7, self.middle.1 + 1),
        ).unwrap();
        println!("{:?} Seconds Left", (self.when - Instant::now()).as_secs());
        stdin().read_line(&mut val).expect("fail");
        if val.starts_with('y') {
            self.score += 1;
        } else {
            self.missed_words.push_str(&format!("{}, ", word));
        }
        if Instant::now() >= self.when {
            execute!(
                stdout(),
                LeaveAlternateScreen,
                SetColors(Colors::new(Blue, Black)),
            ).unwrap();
            println!("================================================================================");
            execute!(
                stdout(),
                SetColors(Colors::new(DarkYellow, Black)),
            ).unwrap();

            println!("\nYour Score is {}!\n", self.score);
            execute!(
                stdout(),
                SetColors(Colors::new(Blue, Black)),
            ).unwrap();
            println!("================================================================================");
            self.missed_words.pop();
            self.missed_words.pop();
            if self.missed_words.len() > 0 {
                println!("\nMissed words:\n");
                execute!(
                    stdout(),
                    SetColors(Colors::new(DarkYellow, Black)),
                ).unwrap();
                println!("{}", self.missed_words);
            } else {
                execute!(
                    stdout(),
                    SetColors(Colors::new(DarkYellow, Black)),
                ).unwrap();
                println!("\nNo Missed words!");
            }
            execute!(
                stdout(),
                SetColors(Colors::new(Blue, Black)),
            ).unwrap();

            Poll::Ready("\n================================================================================")
        } else {
            // Ignore this line for now.
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
//TODO:
//[]  Make numbers into a square
//[]  Up font size on questions
//[]  Use Bracets or hypens to emphasize questions
//[]  Visual Feedback of correct or incorrect. Flash screen?
//[]  Live timer. Tell when last guess

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut word_cloud: Vec<String> = Vec::new();
    String::from_utf8(read(Path::new("files/ASOIAF_list.txt")).expect("file not found!"))
        .expect("Non utf8 symbols found")
        .lines()
        .for_each(|word| word_cloud.push(word.to_string()));

    let x = term_size::dimensions().unwrap_or((64,64));

    let future = Delay {
        when: Instant::now() + Duration::from_millis(args.game_time * 1000  + 200 + 3000), //200 ms added for compute time and 3 seconds added for countdown
        length: word_cloud.len(),
        score: 0,
        word_cloud,
        rng: rand::thread_rng(),
        missed_words: String::new(),
        middle: (x.0 as u16 / 2 ,x.1 as u16 / 2),
    };

    execute!(
        stdout(),
        EnterAlternateScreen,
        terminal::SetTitle("ASOIAF Heads Up"),
        SetColors(Colors::new(White,Blue)),
        Clear(terminal::ClearType::All),
    ).unwrap();

    setup(future, x).await;
}

async fn setup(game: Delay, terminal: (usize, usize)) {
    let row = terminal.0 as u32;
    let col = terminal.1 as u32;
    let render = RenderOptions::new().width(row).height(col).charset(&[" ", ".", ",", "-", "*","$", "#"]);
    execute!(
        stdout(),
        DisableBlinking,
        MoveTo(row as u16 / 2, col as u16 / 2),
    ).unwrap();
    for i in (1..=3).rev() {
        let mut buffer = String::new();
        render_to(format!("files/{}.png", i), &mut  buffer, &render).unwrap();
        println!("{buffer}");
        thread::sleep(SECOND);
        execute!(
            stdout(),
            Clear(terminal::ClearType::All),
            MoveTo(row as u16 / 2, col as u16 / 2),
        ).unwrap();
    }
    println!("{}", game.await);
}
