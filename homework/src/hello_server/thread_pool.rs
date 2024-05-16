//! Thread pool that joins all thread when dropped.

// NOTE: Crossbeam channels are MPMC, which means that you don't need to wrap the receiver in
// Arc<Mutex<..>>. Just clone the receiver and give it to each worker thread.
use crossbeam_channel::{unbounded, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

struct Job(Box<dyn FnOnce() + Send + 'static>);

#[derive(Debug)]
struct Worker {
    _id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for Worker {
    /// When dropped, the thread's `JoinHandle` must be `join`ed.  If the worker panics, then this
    /// function should panic too.
    ///
    /// NOTE: The thread is detached if not `join`ed explicitly.
    fn drop(&mut self) {
        todo!()
    }
}

/// Internal data structure for tracking the current job status. This is shared by worker closures
/// via `Arc` so that the workers can report to the pool that it started/finished a job.
#[derive(Debug, Default)]
struct ThreadPoolInner {
    job_count: Mutex<usize>,
    empty_condvar: Condvar,
}

impl ThreadPoolInner {
    /// Increment the job count.
    fn start_job(&self) {
        let mut count = self.job_count.lock().unwrap();
        *count += 1;
    }

    /// Decrement the job count.
    fn finish_job(&self) {
        let mut count = self.job_count.lock().unwrap();
        *count -= 1;
        if *count == 0 {
            self.empty_condvar.notify_all();
        }
    }

    fn wait_empty(&self) {
        let mut count = self.job_count.lock().unwrap();
        while *count != 0 {
            count = self.empty_condvar.wait(count).unwrap();
        }
    }
}

/// Thread pool.
#[derive(Debug)]
pub struct ThreadPool {
    _workers: Vec<Worker>,
    job_sender: Option<Sender<Job>>,
    pool_inner: Arc<ThreadPoolInner>,
}

impl ThreadPool {
    /// Create a new ThreadPool with `size` threads. Panics if the size is 0.
    pub fn new(size: usize) -> Self {
        assert!(size > 0);
        let (sender, receiver) = unbounded::<Job>();
        let pool_inner = Arc::new(ThreadPoolInner::default());
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            let receiver = receiver.clone();
            let pool_inner_clone = Arc::clone(&pool_inner);
            let thread = thread::spawn(move || {
                while let Ok(job) = receiver.recv() {
                    pool_inner_clone.start_job();
                    (job.0)();
                    pool_inner_clone.finish_job();
                }
            });

            workers.push(Worker {
                _id: id,
                thread: Some(thread),
            });
        }

        ThreadPool {
            _workers: workers,
            job_sender: Some(sender),
            pool_inner,
        }
    }

    /// Execute a new job in the thread pool.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Job(Box::new(f));
        if let Some(sender) = &self.job_sender {
            sender.send(job).expect("Thread pool has no sender");
        }
    }

    /// Block the current thread until all jobs in the pool have been executed.
    ///
    /// NOTE: This method has nothing to do with `JoinHandle::join`.
    pub fn join(&self) {
        self.pool_inner.wait_empty();
    }
}

impl Drop for ThreadPool {
    /// When dropped, all worker threads' `JoinHandle` must be `join`ed. If the thread panicked,
    /// then this function should panic too.
    fn drop(&mut self) {
        drop(self.job_sender.take()); // Drop the sender to close the channel and stop all threads.

        for worker in &mut self._workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().expect("Worker thread panicked");
            }
        }
    }
}
