use crossterm::cursor::MoveToPreviousLine;
use crossterm::{execute, terminal};
use crossterm::terminal::{window_size, Clear, WindowSize};
use rand::rngs::ThreadRng;
use rand::Rng;
use std::fs::read;
use std::future::Future;
use std::io::{stdin, stdout};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use crossterm::style::{Color::{Blue,White}, Colors, SetColors};

struct Delay {
    when: Instant,
    score: usize,
    word_cloud: Vec<String>,
    rng: ThreadRng,
    length: usize,
}

impl Future for Delay {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        let mut val = String::new();
        let s = self.length;
        let picker = self.rng.gen_range(0..s);
        let word = self.word_cloud.get(picker).unwrap();
        execute!(
            stdout(),
            MoveToPreviousLine(3),
            Clear(terminal::ClearType::FromCursorDown),
        ).unwrap();
        println!(
            "{}\n{:?} Seconds left",
            word,
            (self.when - Instant::now()).as_secs()
        );
        stdin().read_line(&mut val).expect("fail");
        if val.starts_with('y') {
            self.score += 1;
        }
        if Instant::now() >= self.when {
            println!("Score is {}", self.score);
            Poll::Ready("time is up")
        } else {
            // Ignore this line for now.
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[tokio::main]
async fn main() {
    let mut word_cloud: Vec<String> = Vec::new();
    String::from_utf8(read(Path::new("files/ASOIAF_list.txt")).expect("file not found!"))
        .expect("Non utf8 symbols found")
        .lines()
        .for_each(|word| word_cloud.push(word.to_string()));
    let future = Delay {
        when: Instant::now() + Duration::from_millis(60020), //20 ms added for compute time
        length: word_cloud.len(),
        score: 0,
        word_cloud,
        rng: rand::thread_rng(),
    };

    let window = match window_size() {
        Ok(x) => {
            println!("--------------------------------------------------------------------------------");
            x
        },
        Err(_) => WindowSize { rows: 20, columns: 20, width: 400, height: 400 }
    };
    execute!(
        stdout(),
        terminal::SetTitle("ASOIAF Heads Up"),
        SetColors(Colors::new(White,Blue)),
        crossterm::terminal::SetSize(50,20),
        crossterm::cursor::MoveTo(window.rows / 2, window.columns / 2),
        Clear(terminal::ClearType::All),
    ).unwrap();
    let out = future.await;
    println!("{}", out);
}
