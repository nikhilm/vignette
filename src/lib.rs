#[cfg(target_os = "linux")]
mod lib_linux;
#[cfg(target_os = "linux")]
pub use lib_linux::*;

// next step is to add IP grabbing using libunwind-sys.

#[cfg(test)]
mod tests {
    extern crate libc;
    use super::*;
    use std::thread::spawn;
    use std::sync::mpsc::channel;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_suspend_resume() {
        let sampler = Sampler::new();
        let (tx, rx) = channel();
        // Just to get the thread to wait until the test is done.
        let (tx2, rx2) = channel();
        let handle = spawn(move || {
            tx.send(get_current_thread()).unwrap();
            rx2.recv().unwrap();
        });

        let to = rx.recv().unwrap();
        sampler.suspend_and_resume_thread(&to, |context| {
            // TODO: This is where we would want to use libunwind in a real program.
            assert!(context.uc_stack.ss_size > 0);

            // we can tell the thread to shutdown once it is resumed.
            tx2.send(()).unwrap();
        });

        handle.join().unwrap();
    }

}
