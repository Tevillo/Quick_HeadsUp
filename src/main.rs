use rand::rngs::ThreadRng;
use rand::Rng;
use std::fs::read;
use std::future::Future;
use std::io::stdin;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

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
    let bytes = String::from_utf8(
        read(Path::new(
            "src/ASOIAF_list.txt",
        ))
        .unwrap(),
    )
    .unwrap();
    let lines = bytes.lines();
    let mut word_cloud: Vec<String> = Vec::new();
    lines.for_each(|x| word_cloud.push(x.to_string()));
    let when = Instant::now() + Duration::from_millis(60020); //20 ms added for compute type
    let rng = rand::thread_rng();
    let length = word_cloud.len();
    let future = Delay {
        when,
        score: 0,
        word_cloud,
        rng,
        length,
    };

    let out = future.await;
    println!("{}", out);
}
