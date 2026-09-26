use std::io::{IsTerminal, Write as _};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use libafl::{
    monitors::{stats::ClientStatsManager, Monitor},
    Error,
};
use libafl_bolts::ClientId;

const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";
const CLEAR_LINE: &str = "\x1b[2K";

const STATUS_INTERVAL_TTY: Duration = Duration::from_millis(300);
const STATUS_INTERVAL_PIPE: Duration = Duration::from_millis(2000);

fn colors_enabled() -> &'static bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    ENABLED.get_or_init(|| {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        !no_color && std::io::stdout().is_terminal()
    })
}

fn paint(code: &str, text: &str) -> String {
    if *colors_enabled() {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn dim(text: &str) -> String {
    paint(DIM, text)
}

pub fn bold(text: &str) -> String {
    paint(BOLD, text)
}

pub fn cyan(text: &str) -> String {
    paint(CYAN, text)
}

pub fn green(text: &str) -> String {
    paint(GREEN, text)
}

pub fn red(text: &str) -> String {
    let enabled = std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
        && std::io::stderr().is_terminal();
    if enabled {
        format!("{RED}{text}{RESET}")
    } else {
        text.to_string()
    }
}

/// Print a full line, clearing the in-place status line first if there is one.
pub fn line(text: &str) {
    if *colors_enabled() {
        println!("{CLEAR_LINE}\r{text}");
    } else {
        println!("{text}");
    }
}

/// Compact count, for example 1234 -> "1.2k".
#[must_use]
pub fn human(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format!("{:.1}k", count as f64 / 1e3),
        1_000_000..=999_999_999 => format!("{:.1}M", count as f64 / 1e6),
        _ => format!("{:.1}G", count as f64 / 1e9),
    }
}

fn segment(label: &str, value: &str) -> String {
    format!(" {} {} {}", dim("·"), dim(label), value)
}

/// Live status line for the fuzzer: redraws in place on a terminal.
#[derive(Default)]
pub struct TalonMonitor {
    last_draw: Option<Instant>,
}

impl Monitor for TalonMonitor {
    fn display(
        &mut self,
        client_stats_manager: &mut ClientStatsManager,
        event_msg: &str,
        sender_id: ClientId,
    ) -> Result<(), Error> {
        match event_msg {
            "UserStats" | "Client Heartbeat" => self.draw_status(client_stats_manager, sender_id),
            "Objective" => self.draw_crash(client_stats_manager),
            _ => Ok(()),
        }
    }
}

impl TalonMonitor {
    fn draw_status(
        &mut self,
        client_stats_manager: &mut ClientStatsManager,
        sender_id: ClientId,
    ) -> Result<(), Error> {
        let interval = if *colors_enabled() {
            STATUS_INTERVAL_TTY
        } else {
            STATUS_INTERVAL_PIPE
        };
        if let Some(last) = self.last_draw {
            if last.elapsed() < interval {
                return Ok(());
            }
        }
        self.last_draw = Some(Instant::now());

        let (run_time, total_execs, exec_rate, corpus, objectives) = {
            let stats = client_stats_manager.global_stats();
            (
                stats.run_time_pretty.clone(),
                stats.total_execs,
                stats.execs_per_sec_pretty.clone(),
                stats.corpus_size,
                stats.objective_size,
            )
        };
        let signals = {
            client_stats_manager.client_stats_insert(sender_id)?;
            client_stats_manager
                .client_stats_for(sender_id)?
                .user_stats()
                .get("signals")
                .map_or_else(String::new, |stat| stat.to_string())
        };

        let mut text = format!("{} {}", cyan("▸"), run_time);
        text.push_str(&segment("execs", &human(total_execs)));
        text.push_str(&segment("rate", &exec_rate));
        text.push_str(&segment("corpus", &corpus.to_string()));
        text.push_str(&segment("obj", &objectives.to_string()));
        if !signals.is_empty() {
            text.push_str(&segment("sig", &signals));
        }

        if *colors_enabled() {
            print!("{CLEAR_LINE}\r{text}");
            std::io::stdout().flush().ok();
        } else {
            println!("{text}");
        }
        Ok(())
    }

    fn draw_crash(&mut self, client_stats_manager: &mut ClientStatsManager) -> Result<(), Error> {
        let (run_time, total_execs, corpus) = {
            let stats = client_stats_manager.global_stats();
            (
                stats.run_time_pretty.clone(),
                stats.total_execs,
                stats.corpus_size,
            )
        };
        let execs = human(total_execs);
        let text = format!(
            "{} {} {} {} execs, corpus {corpus}",
            green("✓ crash found"),
            dim("·"),
            run_time,
            execs
        );
        line(&text);
        Ok(())
    }
}
