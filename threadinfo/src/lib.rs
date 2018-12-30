/// threadinfo is a library to retrieve OS threads and related information in a platform indepedent
/// manner. Currently it provides:
/// - An iterator over the current process' threads.
/// threadinfo explicitly deals with OS threads, even if such threads may be programmed against
/// using an abstraction like pthreads. This is because the functionality it provides/intends to
/// provide, uses OS APIs that rely on those details. It uses the Windows Threads API, Mach and
/// Linux tasks.

#[macro_use]
extern crate serde_derive;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use self::linux::*;

#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(target_os = "macos")]
pub use self::mac::*;

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc,Barrier, Mutex};
    use super::{current_thread, thread_iterator};

    #[test]
    fn current_thread_test() {
        current_thread().expect("thread");
    }

    #[test]
    fn thread_iterator_one() {
        let curr = current_thread().expect("current thread");
        assert!(thread_iterator().expect("iterator").any(|it| it == curr));
    }

    #[test]
    fn thread_iterator_multiple() {
        let mut threads = Vec::with_capacity(10);
        let ids = Arc::new(Mutex::new(HashSet::new()));
        let ids_barrier = Arc::new(Barrier::new(threads.capacity() + 1));
        let terminate_barrier = Arc::new(Barrier::new(threads.capacity() + 1));
        for _i in 0..threads.capacity() {
            let ids2 = ids.clone();
            let ids_barrier2 = ids_barrier.clone();
            let terminate_barrier2 = terminate_barrier.clone();
            threads.push(std::thread::spawn(move || {
                ids2.lock().unwrap().insert(current_thread().unwrap());
                ids_barrier2.wait();
                terminate_barrier2.wait();
            }));
        }

        ids_barrier.wait();

        let its: HashSet<_> = thread_iterator().unwrap().collect();
        assert!(its.len() >= threads.capacity() + 1);
        assert!(its.is_superset(&ids.lock().unwrap()));

        terminate_barrier.wait();
        for thread in threads {
            thread.join().unwrap();
        }
    }
}
