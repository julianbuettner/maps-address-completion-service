use std::{
    io::{BufReader, Read, Seek, Write},
    sync::{
        mpsc::{channel, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread::{spawn, JoinHandle},
    time::{Duration, Instant},
};

use log::info;

#[derive(Clone)]
pub struct VerboseReaderManager {
    filesize: usize,
    position_slow: Arc<Mutex<usize>>,
    output_thread: Arc<Mutex<Option<(JoinHandle<()>, Sender<()>)>>>,
    seek_count: Arc<Mutex<usize>>,
    last_update: Instant,
    update_interval_: Duration,
}

pub struct VerboseReader<R: Read> {
    filesize: usize,
    inner: BufReader<R>,
    position_fast: usize,
    update_count: usize,
    seek_count: Arc<Mutex<usize>>,
    position_slow: Arc<Mutex<usize>>,
    output_thread: Arc<Mutex<Option<(JoinHandle<()>, Sender<()>)>>>,
}

fn printing_thread(manager: VerboseReaderManager, kill_receiver: Receiver<()>) {
    // loop while not receiving kill message.
    // Terminate for kill message or for dropped sender
    while let Err(RecvTimeoutError::Timeout) = kill_receiver.recv_timeout(manager.update_interval_)
    {
        let pass = { *manager.seek_count.lock().unwrap() };
        let position = { *manager.position_slow.lock().unwrap() / 1024 / 1024 };
        let filesize = manager.filesize / 1024 / 1024;
        let percent = position as f32 / filesize as f32;
        info!(
            "Pass {} | Read {}MiB / {}MiB, {:.1}%",
            pass,
            position,
            filesize,
            percent * 100.
        );
    }
}

impl VerboseReaderManager {
    pub fn print_interval(mut self, dur: Duration) -> Self {
        self.update_interval_ = dur;
        self
    }
    pub fn stop_printing(&self) {
        match { self.output_thread.lock().unwrap().take() } {
            None => (),
            Some((join_handle, killer)) => {
                killer.send(()).unwrap();
                join_handle.join().unwrap();
            }
        }
    }
    pub fn start_printing(&self) {
        let (send, recv) = channel();
        let self_clone = self.clone();
        let join_handle = spawn(|| printing_thread(self_clone, recv));
        self.stop_printing();
        let _none = self
            .output_thread
            .lock()
            .unwrap()
            .insert((join_handle, send));
    }
}

impl<R: Read> VerboseReader<R> {
    fn increase(&mut self, increase: usize) {
        self.position_fast += increase;
        self.update_count += 1;
        if self.update_count % 50 == 0 {
            self.update_count = 0;
            let mut count_slow = self.position_slow.lock().unwrap();
            *count_slow = self.position_fast
        }
    }

    pub fn new(inner: R, filesize: usize) -> (Self, VerboseReaderManager) {
        let obj = Self {
            filesize,
            position_fast: 0,
            seek_count: Default::default(),
            inner: BufReader::new(inner),
            position_slow: Default::default(),
            update_count: Default::default(),
            output_thread: Default::default(),
        };
        let manager = VerboseReaderManager {
            output_thread: obj.output_thread.clone(),
            position_slow: (obj.position_slow.clone()),
            seek_count: obj.seek_count.clone(),
            last_update: Instant::now(),
            filesize,
            update_interval_: Duration::from_secs(5),
        };
        (obj, manager)
    }
}

impl<R: Read> Read for VerboseReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let res = self.inner.read(buf);
        if let Ok(count) = res {
            self.increase(count)
        }
        res
    }
}

impl<R: Read + Seek> Seek for VerboseReader<R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Start(s) => self.position_fast = s as usize,
            std::io::SeekFrom::End(e) => {
                self.position_fast = (self.filesize as i64 - e as i64) as usize
            }
            std::io::SeekFrom::Current(c) => {
                self.position_fast = (self.position_fast as i64 + c) as usize
            }
        }
        *self.seek_count.lock().unwrap() += 1;
        self.inner.seek(pos)
    }
}
