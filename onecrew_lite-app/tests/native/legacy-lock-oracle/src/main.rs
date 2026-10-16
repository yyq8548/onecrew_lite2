use std::io::{Read, Write};

fn main() {
    let path = std::env::args_os().nth(1).expect("lock path required");
    let mut lock = fslock::LockFile::open(&path).expect("open native lock");
    assert!(lock.try_lock().expect("try native lock"), "native owner already running");
    println!("locked");
    std::io::stdout().flush().expect("flush lock receipt");
    let mut byte = [0_u8; 1];
    std::io::stdin().read_exact(&mut byte).expect("release signal");
}
