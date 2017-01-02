#![feature(integer_atomics)]

#[macro_use] extern crate log;
extern crate crossbeam;

use std::thread;

// use std::sync::{Arc, Mutex};
use std::sync::{self, Arc, Mutex};
use std::cell::{Cell, RefCell};
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
    stats: Arc<Mutex<DirStats>>,
}

// From riggrep/ignore/src/walk.rs

#[derive(Debug)]
struct DirStats {
    depth: u32,
    path: PathBuf,
    bytes: Cell<u64>,
    // child_dirs: AtomicU64,
    children: RefCell<Vec<Arc<Mutex<DirStats>>>>,
    // parent_weak: RefCell<Option<sync::Weak<DirStats>>>,
}

impl DirStats {

    // fn read_dir(&mut self) -> io::Result<fs::ReadDir> {
    //     Ok(fs::read_dir(&self.path)?)
    // }

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
        let stats = work.stats.lock().unwrap();
        for result in stats.path.read_dir()? {
            let dent = result?;
            let metadata = dent.metadata()?;
            if metadata.is_file() {
                bytes += metadata.len();
            } else if metadata.is_dir() {
                let s = Arc::new(Mutex::new(DirStats {
                    depth: stats.depth + 1,
                    path: PathBuf::from(dent.path()),
                    bytes: Cell::new(0),
                    children: RefCell::new(vec![]),
                }));
                stats.children.borrow_mut().push(s.clone());
                self.queue.push(Message::Work(Work { stats: s }));
            }
        }

        stats.bytes.set(bytes);
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

fn print(dir: Arc<Mutex<DirStats>>) -> u64 {
    let stats = dir.lock().unwrap();
    let mut bytes = 0;
    // TODO: Use into_inner()?
    for d in stats.children.borrow().iter() {
        bytes += print(d.clone());
    }
    let total = stats.bytes.get() + bytes;
    println!("{:?}: {}", stats.path, total);
    total
}

pub fn run(dir: &str) {
    // TODO: use num_cpus
    let threads = 4;

    let queue = Arc::new(MsQueue::new());

    let cur = Arc::new(Mutex::new(DirStats {
        depth: 0,
        path: PathBuf::from(dir),
        bytes: Cell::new(0),
        children: RefCell::new(vec![]),
    }));

    let work = Work {
        stats: cur.clone(),
    };

    queue.push(Message::Work(work));
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
    print(cur);
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
