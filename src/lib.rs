#![feature(integer_atomics)]

#[macro_use] extern crate log;
extern crate crossbeam;

use std::thread;

// use std::sync::{Arc, Mutex};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicU64, Ordering};
use crossbeam::sync::MsQueue;
use std::path::{Path, PathBuf};

use std::io;
// use std::fs;

#[derive(Debug)]
enum Message {
    Work(Work),
    Quit,
}

#[derive(Debug)]
struct Work {
    depth: u32,
    path: PathBuf,
    parent: Option<Arc<DirStats>>,
}

// From riggrep/ignore/src/walk.rs

#[derive(Debug)]
struct DirStats {
    path: PathBuf,
    bytes: AtomicU64,
    child_dirs: AtomicU64,
    parent: Option<Arc<DirStats>>,

    // children: Vec<DirStats>,
    // parent_weak: Weak:
}

impl DirStats {

    // fn read_dir(&mut self) -> io::Result<fs::ReadDir> {
    //     Ok(fs::read_dir(&self.path)?)
    // }

    fn print(&self) {
        println!("{:?}: {}", self.path, self.bytes.load(Ordering::SeqCst));
    }
}

struct Worker {
    id: u32,
    queue: Arc<MsQueue<Message>>,
    quit_now: Arc<AtomicBool>,
    is_waiting: bool,
    is_quitting: bool,
    num_waiting: Arc<AtomicUsize>,
    num_quitting: Arc<AtomicUsize>,
    threads: usize,
}

fn print(path: &Path, bytes: u64) {
    println!("{:?}: {}", path, bytes);
}

impl Worker {
    pub fn run(mut self) {
        debug!("id: {}, run", self.id);
        while let Some(work) = self.get_work() {
            debug!("id: {}, work: {:?}", self.id, work);
            if let Err(_) = self.run_one(work) {
                self.quit_now();
                return;
            }
        }
    }

    fn run_one(&mut self, work: Work) -> io::Result<()> {
        let mut bytes = 0;
        let mut child_dirs = vec![];
        for result in work.path.read_dir()? {
            let dent = result?;
            let metadata = dent.metadata()?;
            if metadata.is_file() {
                bytes += metadata.len();
            } else if metadata.is_dir() {
                child_dirs.push(dent.path());
            }
        }
        if !child_dirs.is_empty() {
            let current = Arc::new(DirStats {
                path: work.path.clone(),
                bytes: AtomicU64::new(bytes),
                child_dirs: AtomicU64::new(child_dirs.len() as u64),
                parent: work.parent.clone(),
            });
            for d in child_dirs {
                self.queue.push(Message::Work(Work {
                    depth: work.depth + 1,
                    path: d,
                    parent: Some(current.clone()),
                }));
            }
        } else {
            print(&work.path, bytes);
        }

        let mut work = work;

        // TODO: Recursivly print parent.
        let mut cur = &mut work.parent.as_mut();
        // let mut cur = &mut work.parent;

        while let Some(ref mut parent) = *cur {
            parent.bytes.fetch_add(bytes, Ordering::SeqCst);
            if parent.child_dirs.fetch_sub(1, Ordering::SeqCst) != 1 {
                break;
            }
            parent.print();
            bytes = parent.bytes.load(Ordering::SeqCst);
            // cur = &mut parent.parent.as_mut();
        }
        Ok(())
    }

    fn get_work(&mut self) -> Option<Work> {
        loop {
            if self.is_quit_now() {
                return None;
            }

            match self.queue.try_pop() {
                Some(Message::Work(work)) => {
                    self.waiting(false);
                    self.quitting(false);
                    return Some(work);
                }
                Some(Message::Quit) => {
                    self.waiting(true);
                    self.quitting(true);
                    debug!("id: {}, receive quit", self.id);
                    while !self.is_quit_now() {
                        let nwait = self.num_waiting();
                        let nquit = self.num_quitting();
                        // If the number of waiting workers dropped, then
                        // abort our attempt to quit.
                        if nwait < self.threads {
                            break;
                        }
                        // If all workers are in this quit loop, then we
                        // can stop.
                        if nquit == self.threads {
                            return None;
                        }
                        // Otherwise, spin.
                    }
                    // If we're here, then we've aborted our quit attempt.
                    continue;
                }
                None => {
                    debug!("id: {}, get none", self.id);
                    self.waiting(true);
                    self.quitting(false);
                    if self.num_waiting() == self.threads {
                        debug!("id: {}, push Quit", self.id);
                        for _ in 0..self.threads {
                            self.queue.push(Message::Quit);
                        }
                    }
                    continue;
                }
            }
        }
    }

    /// Indicates that all workers should quit immediately.
    fn quit_now(&self) {
        self.quit_now.store(true, Ordering::SeqCst);
    }

    /// Returns true if this worker should quit immediately.
    fn is_quit_now(&self) -> bool {
        self.quit_now.load(Ordering::SeqCst)
    }

    /// Returns the total number of workers waiting for work.
    fn num_waiting(&self) -> usize {
        self.num_waiting.load(Ordering::SeqCst)
    }

    /// Returns the total number of workers ready to quit.
    fn num_quitting(&self) -> usize {
        self.num_quitting.load(Ordering::SeqCst)
    }

    /// Sets this worker's "quitting" state to the value of `yes`.
    fn quitting(&mut self, yes: bool) {
        if yes {
            if !self.is_quitting {
                self.is_quitting = true;
                self.num_quitting.fetch_add(1, Ordering::SeqCst);
            }
        } else {
            if self.is_quitting {
                self.is_quitting = false;
                self.num_quitting.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    /// Sets this worker's "waiting" state to the value of `yes`.
    fn waiting(&mut self, yes: bool) {
        if yes {
            if !self.is_waiting {
                self.is_waiting = true;
                self.num_waiting.fetch_add(1, Ordering::SeqCst);
            }
        } else {
            if self.is_waiting {
                self.is_waiting = false;
                self.num_waiting.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }
}

pub fn run(dir: &str) {
    // TODO: use num_cpus
    let threads = 4;

    let queue = Arc::new(MsQueue::new());

    queue.push(Message::Work(Work {
        depth: 0,
        path: PathBuf::from(dir),
        parent: None,
    }));
    let num_waiting = Arc::new(AtomicUsize::new(0));
    let num_quitting = Arc::new(AtomicUsize::new(0));
    let quit_now = Arc::new(AtomicBool::new(false));

    let mut handles = vec![];
    for id in 0..threads {
        let worker = Worker {
            id: id,
            queue: queue.clone(),
            quit_now: quit_now.clone(),
            is_waiting: false,
            is_quitting: false,
            num_waiting: num_waiting.clone(),
            num_quitting: num_quitting.clone(),
            threads: threads as usize,
        };
        handles.push(thread::spawn(|| worker.run()));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
