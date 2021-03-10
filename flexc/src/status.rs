use std::sync::atomic::*;
use std::sync::Arc;

pub(crate) const STATUS_EMPTY: u8 = 0;
pub(crate) const STATUS_INCHECK: u8 = 1;
pub(crate) const STATUS_INUSE: u8 = 2;
pub(crate) const STATUS_IDLE: u8 = 3;
#[derive(Clone, Debug)]
pub(crate) struct Status(pub(crate) Arc<Vec<AtomicU8>>);

impl Status {
    pub fn new(maxsize: usize) -> Self {
        let this = (0..maxsize)
            .into_iter()
            .map(|_| AtomicU8::new(STATUS_EMPTY))
            .collect::<Vec<_>>();
        Self(Arc::new(this))
    }

    pub fn set_empty(&self, idx: usize) {
        self.0[idx].store(STATUS_EMPTY, Ordering::SeqCst)
    }

    pub fn set_incheck(&self, idx: usize) {
        self.0[idx].store(STATUS_INCHECK, Ordering::SeqCst)
    }

    pub fn is_incheck(&self, idx: usize) -> bool {
        self.0[idx].load(Ordering::Relaxed) == STATUS_INCHECK
    }

    pub fn set_inuse(&self, idx: usize) {
        self.0[idx].store(STATUS_INUSE, Ordering::SeqCst)
    }

    pub fn set_idle(&self, idx: usize) {
        self.0[idx].store(STATUS_IDLE, Ordering::SeqCst)
    }

    pub fn state(&self) -> State {
        let mut state = State::default();
        state.maxsize = self.0.len() as _;

        for (idx, s) in self.0.as_slice().iter().enumerate() {
            let s = s.load(Ordering::Relaxed);
            match s {
                STATUS_EMPTY => state.empty += 1,
                STATUS_INCHECK => state.incheck += 1,
                STATUS_INUSE => state.inuse += 1,
                STATUS_IDLE => state.idle += 1,
                invalid => unreachable!("conn-{} invalid status: {}", idx, invalid),
            }
        }
        state.size = state.inuse + state.idle + state.incheck;
        state.maxsize = state.size + state.empty;
        state.wait = Arc::weak_count(&self.0) as _;

        state
    }
}

#[derive(Default, Clone, Debug, PartialEq, PartialOrd)]
/// Information about the state of a `Pool`.
pub struct State {
    /// Maximum number of open connections to the database
    pub maxsize: u32,
    /// Number of unconnected
    pub empty: u32,

    // Pool Status
    /// The number of established connections both in use, in check and idle
    pub size: u32,
    /// The number of connections currently in use
    pub inuse: u32,
    /// The number of idle connections
    pub idle: u32,
    /// The number of connections currently in check
    pub incheck: u32,

    /// The total number of connections waited for
    pub wait: u32,
}
