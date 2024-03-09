use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{EventStream, KeyCode, KeyEvent, KeyEventKind};
use futures::{
    stream::{self, StreamExt},
    TryStreamExt,
};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use tap::prelude::*;
use tokio::time::{Instant, Interval};
use tokio_stream::wrappers::IntervalStream;

#[allow(clippy::unnecessary_wraps)]
#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let config: Config = std::fs::read_to_string("config.toml")?.pipe_deref(toml::from_str)?;
    loop {
        for _ in 0..4 {
            let should_quit = timer_bar(
                config.work_time,
                Duration::from_millis(25).pipe(tokio::time::interval),
                "Working\n{elapsed_precise} {wide_bar:.cyan/blue}\n{msg}",
            )
            .await?;
            if should_quit {
                return Ok(());
            }
            let should_quit = timer_bar(
                config.short_break,
                Duration::from_millis(25).pipe(tokio::time::interval),
                "Short Break\n{elapsed_precise} {wide_bar:.cyan/blue}\n{msg}",
            )
            .await?;
            if should_quit {
                return Ok(());
            }
        }
        let should_quit = timer_bar(
            config.long_break,
            Duration::from_millis(25).pipe(tokio::time::interval),
            "Long Break\n{elapsed_precise} {wide_bar:.cyan/blue}\n{msg}",
        )
        .await?;
        if should_quit {
            return Ok(());
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct Config {
    #[serde(deserialize_with = "deserialize_duration")]
    work_time: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    short_break: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    long_break: Duration,
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let s = match toml::Value::deserialize(deserializer)? {
        toml::Value::String(s) => s,
        toml::Value::Float(f) => return Duration::from_secs_f64(f).pipe(Ok),
        toml::Value::Integer(i) => {
            return i
                .try_conv::<u64>()
                .map_err(D::Error::custom)
                .map(Duration::from_secs)
        }
        _ => return D::Error::custom("Expected a string or a number").pipe(Err),
    };

    match s
        .to_lowercase()
        .split_once(' ')
        .map(|(first, second)| (first, second.trim()))
    {
        Some((h, "h")) => h
            .parse::<u64>()
            .map_err(D::Error::custom)
            .map(|s| s * 3_600)
            .map(Duration::from_secs),
        Some((m, "m")) => m
            .parse::<u64>()
            .map_err(D::Error::custom)
            .map(|s| s * 60)
            .map(Duration::from_secs),
        Some((s, "s")) => s.parse().map(Duration::from_secs).map_err(D::Error::custom),
        Some(_) => todo!(),
        None => s.parse().map(Duration::from_secs).map_err(D::Error::custom),
    }
}

async fn timer_bar(duration: Duration, interval: Interval, bar_template: &str) -> Result<bool> {
    let bar = {
        let bar = duration
            .as_millis()
            .try_conv::<u64>()?
            .pipe(ProgressBar::new);
        bar.set_style(ProgressStyle::with_template(bar_template)?);
        bar.set_message("");
        bar
    };
    let start = Instant::now();
    let mut command = String::new();

    let mut events = Event::stream(interval);
    while let Some(evt) = events.try_next().await? {
        match evt {
            Event::Tick => {
                let dt = Instant::now() - start;
                bar.set_position(dt.as_millis().try_into().expect("todo"));
                if dt > duration {
                    break;
                } else {
                    continue;
                };
            }
            Event::KeyPressed(KeyCode::Enter) => match command.as_str() {
                ":q" | ":quit" => {
                    bar.finish_and_clear();
                    return Ok(true);
                }
                _ => {
                    command.clear();
                }
            },
            Event::KeyPressed(KeyCode::Esc) => {
                command.clear();
            }
            Event::KeyPressed(KeyCode::Char(':')) | Event::KeyRepeated(KeyCode::Char(':')) => {
                command.push(':');
            }
            Event::KeyPressed(KeyCode::Char(c)) | Event::KeyRepeated(KeyCode::Char(c))
                if !command.is_empty() =>
            {
                command.push(c);
            }
            Event::KeyPressed(KeyCode::Backspace) | Event::KeyRepeated(KeyCode::Backspace) => {
                command.pop();
            }
            _ => continue,
        }
        bar.set_message(command.clone());
    }
    bar.finish_and_clear();
    Ok(false)
}

enum Event {
    Tick,
    KeyPressed(KeyCode),
    KeyReleased(KeyCode),
    KeyRepeated(KeyCode),
}

impl Event {
    fn stream(interval: Interval) -> impl futures::Stream<Item = std::io::Result<Self>> {
        stream::select(
            interval.pipe(IntervalStream::new).map(|_| Ok(Self::Tick)),
            EventStream::new()
                .map_ok(Self::from_crossterm)
                .filter_map(|res| res.transpose().pipe(futures::future::ready)),
        )
    }

    #[allow(clippy::needless_pass_by_value)]
    fn from_crossterm(inner: crossterm::event::Event) -> Option<Self> {
        match inner {
            crossterm::event::Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) => Self::KeyPressed(code).pipe(Some),
            crossterm::event::Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Release,
                ..
            }) => Self::KeyReleased(code).pipe(Some),
            crossterm::event::Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Repeat,
                ..
            }) => Self::KeyRepeated(code).pipe(Some),
            _ => None,
        }
    }
}
