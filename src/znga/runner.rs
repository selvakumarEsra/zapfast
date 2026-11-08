//! Znga's worker thread: model calls never run on the interface thread.
//! Requests run one at a time, in order; each answer wakes the window.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use super::provider::{self, Prompt, Target};

type Request = (u64, Target, Prompt);
type Answer = (u64, Result<String, String>);
#[cfg(test)]
type Fake = Box<dyn Fn(&Prompt) -> Result<String, String>>;

pub struct Runner<P> {
    sender: Option<Sender<Request>>,
    answers: Option<Receiver<Answer>>,
    waker: crate::backend::Waker,
    next: u64,
    pending: HashMap<u64, P>,
    /// Test hook: answers requests in place of the network.
    #[cfg(test)]
    pub fake: Option<Fake>,
    #[cfg(test)]
    fake_answers: Vec<Answer>,
}

/// More queued requests than this and new optional ones are dropped.
pub const BUSY: usize = 6;

impl<P> Runner<P> {
    pub fn new(waker: crate::backend::Waker) -> Self {
        Self {
            sender: None,
            answers: None,
            waker,
            next: 0,
            pending: HashMap::new(),
            #[cfg(test)]
            fake: None,
            #[cfg(test)]
            fake_answers: Vec::new(),
        }
    }

    pub fn busy(&self) -> usize {
        self.pending.len()
    }

    pub fn is_waiting(&self, matches: impl Fn(&P) -> bool) -> bool {
        self.pending.values().any(matches)
    }

    /// Queues `prompt`; its answer comes back from [`Self::poll`] with `purpose`.
    pub fn submit(&mut self, target: Target, prompt: Prompt, purpose: P) -> u64 {
        self.next += 1;
        let id = self.next;
        self.pending.insert(id, purpose);
        #[cfg(test)]
        if let Some(fake) = &self.fake {
            let answer = fake(&prompt);
            self.fake_answers.push((id, answer));
            return id;
        }
        if self.sender.is_none() {
            self.start();
        }
        let sent = self
            .sender
            .as_ref()
            .is_some_and(|sender| sender.send((id, target.clone(), prompt.clone())).is_ok());
        if !sent {
            // The thread is gone: start a new one and try once more.
            self.start();
            if let Some(sender) = &self.sender {
                let _ = sender.send((id, target, prompt));
            }
        }
        id
    }

    fn start(&mut self) {
        let (requests, inbox) = channel::<Request>();
        let (outbox, answers) = channel::<Answer>();
        let waker = self.waker.clone();
        let spawned = std::thread::Builder::new()
            .name("znga".into())
            .spawn(move || {
                for (id, target, prompt) in inbox {
                    let answer = provider::complete(&target, &prompt);
                    if outbox.send((id, answer)).is_err() {
                        break;
                    }
                    waker.wake();
                }
            });
        match spawned {
            Ok(_) => {
                self.sender = Some(requests);
                self.answers = Some(answers);
            }
            Err(_) => log::warn!("znga: could not start its worker thread"),
        }
    }

    /// Answers that arrived, with their purposes.
    pub fn poll(&mut self) -> Vec<(P, Result<String, String>)> {
        let mut out = Vec::new();
        #[cfg(test)]
        for (id, answer) in std::mem::take(&mut self.fake_answers) {
            if let Some(purpose) = self.pending.remove(&id) {
                out.push((purpose, answer));
            }
        }
        let Some(answers) = &self.answers else {
            return out;
        };
        loop {
            match answers.try_recv() {
                Ok((id, answer)) => {
                    if let Some(purpose) = self.pending.remove(&id) {
                        out.push((purpose, answer));
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.sender = None;
                    self.answers = None;
                    for (_, purpose) in self.pending.drain() {
                        out.push((purpose, Err("Znga's worker stopped.".into())));
                    }
                    break;
                }
            }
        }
        out
    }
}
