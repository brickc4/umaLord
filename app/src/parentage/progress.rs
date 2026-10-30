//! What the planner is doing while it makes a team plan, and why: stages that tick off with a
//! line or two each of what they found, sent to the page as they happen and kept with the plan
//! ("How this plan was chosen"). A build checks for Cancel between steps.

use crate::App;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Instant;

/// One stage of a build: its title, what it found, and how long it took (none while running).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Stage {
    pub title: String,
    pub lines: Vec<String>,
    pub secs: Option<f64>,
}

/// What a build reports with: the stages so far, where to send them, and the Cancel flag.
pub struct Progress {
    pub stages: Vec<Stage>,
    started: Instant,
    tx: Option<Sender<Vec<Stage>>>,
    cancel: Arc<AtomicBool>,
}

/// The stages a Champions Meeting build reports, in order (the page lists the ones to come).
pub const CM_STAGES: [&str; 8] = [
    "Reading your account",
    "Choosing runners and planning their parents",
    "Decks and skills",
    "Racing the runners head to head",
    "Comparing with today's team",
    "Ordering the parent careers",
    "Writing up each career",
    "Looking for ways to raise the ceiling",
];

/// The stages a Team Trials build reports, in order.
pub const TT_STAGES: [&str; 8] = [
    "Reading your account",
    "Finding who can run each race",
    "Racing the candidates head to head",
    "Picking three runners per race",
    "Planning parents",
    "Comparing with today's team",
    "Writing up each career",
    "Looking for ways to raise the ceiling",
];

/// What a cancelled build returns.
pub const CANCELLED: &str = "Making a plan was cancelled; nothing changed.";

impl Progress {
    pub fn new(tx: Option<Sender<Vec<Stage>>>, cancel: Arc<AtomicBool>) -> Self {
        Progress { stages: Vec::new(), started: Instant::now(), tx, cancel }
    }

    fn send(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(self.stages.clone());
        }
    }

    /// Close the running stage with its time.
    fn close(&mut self) {
        let now = self.started.elapsed().as_secs_f64();
        let begun: f64 = self.stages.iter().filter_map(|s| s.secs).sum();
        if let Some(s) = self.stages.last_mut().filter(|s| s.secs.is_none()) {
            s.secs = Some((now - begun).max(0.0));
        }
    }

    /// Every stage closed, for keeping with the plan.
    pub fn finish(mut self) -> Vec<Stage> {
        self.close();
        self.send();
        self.stages
    }
}

impl App {
    /// Begin a build's next stage (closing the last), or stop if the player cancelled. Does
    /// nothing outside a build.
    pub(crate) fn stage(&mut self, title: impl Into<String>) -> Result<(), String> {
        self.check_cancel()?;
        if let Some(p) = &mut self.progress {
            p.close();
            p.stages.push(Stage { title: title.into(), ..Default::default() });
            p.send();
        }
        Ok(())
    }

    /// Add what the running stage found.
    pub(crate) fn note(&mut self, line: impl Into<String>) {
        if let Some(p) = &mut self.progress {
            if p.stages.is_empty() {
                p.stages.push(Stage::default());
            }
            if let Some(s) = p.stages.last_mut() {
                s.lines.push(line.into());
            }
            p.send();
        }
    }

    /// Stop with `CANCELLED` if the player pressed Cancel.
    pub(crate) fn check_cancel(&self) -> Result<(), String> {
        match &self.progress {
            Some(p) if p.cancel.load(Ordering::Relaxed) => Err(CANCELLED.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_close_in_order_and_cancel_stops() {
        let mut app = App::for_tests();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        app.progress = Some(Progress::new(Some(tx), cancel.clone()));
        app.stage("Reading your account").unwrap();
        app.note("57 umas");
        app.stage("Choosing runners").unwrap();
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(app.stage("Racing"), Err(CANCELLED.to_string()));
        let stages = app.progress.take().unwrap().finish();
        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0].lines, vec!["57 umas".to_string()]);
        assert!(stages.iter().all(|s| s.secs.is_some()));
        // The page saw each update, the last one complete.
        assert_eq!(rx.try_iter().last().map(|s| s.len()), Some(2));
    }
}
