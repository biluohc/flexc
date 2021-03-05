use flexc_redis::{flexc::Builder, Pool, RedisConnectionManager};
use std::sync::{atomic::*, Arc};
pub use std::time::*;
#[derive(Debug, Clone)]
pub struct Counter(Arc<AtomicUsize>);

impl Counter {
    pub fn new() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }
    #[inline]
    pub fn counter(&self) -> usize {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

#[allow(dead_code)]
async fn get_redis_pool(url: &str, builder: Builder) -> Arc<Pool> {
    let manager = RedisConnectionManager::new(url).unwrap();
    Arc::new(builder.build(manager))
}

#[cfg(any(feature = "tokio-rt", feature = "tokio-rt-tls"))]
pub use tokio::{runtime, task::spawn, time::delay_for as sleep};

#[cfg(any(feature = "tokio-rt", feature = "tokio-rt-tls"))]
pub fn block_on(fut: impl std::future::Future<Output = ()>) {
    let mut rt = runtime::Builder::new()
        .enable_all()
        .threaded_scheduler()
        .build()
        .unwrap();
    rt.block_on(async { assert!(std::env::args().count() >= 1) });
    rt.block_on(fut)
}

#[cfg(any(feature = "async-rt", feature = "async-rt-tls"))]
pub use async_std::task::{block_on as block_on_, sleep, spawn};

#[cfg(any(feature = "async-rt", feature = "async-rt-tls"))]
pub fn block_on(fut: impl std::future::Future<Output = ()>) {
    block_on_(async { assert!(std::env::args().count() >= 1) });
    block_on_(fut)
}
