/// This definition will evolve as we go along.
#[derive(Debug, Hash)]
pub struct Frame {
    #[cfg(target_pointer_width = "32")]
    pub ip: u32,
    #[cfg(target_pointer_width = "64")]
    pub ip: u64,
}

pub type Sample = Vec<Frame>;

// TODO: Generalize i32 error type to use failure.
pub trait Unwinder<T> {
    /// Unwind a stack from a context.
    ///
    /// Returns the collected frames.
    fn unwind(self, context: T) -> Result<Sample, i32>;
}
