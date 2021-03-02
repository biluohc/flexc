use flexc_redis::{Builder, Pool, RedisConnectionManager};
use std::sync::{atomic::*, Arc};
pub use std::time::*;
pub use tokio::{
    runtime::{self, Runtime},
    task::spawn,
    time::delay_for as sleep,
};
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

#[allow(dead_code)]
fn tokio_rt(cpus: usize) -> Runtime {
    if cpus == 0 {
        runtime::Builder::new()
            .enable_all()
            .basic_scheduler()
            .build()
            .unwrap()
    } else {
        runtime::Builder::new()
            .enable_all()
            .threaded_scheduler()
            .core_threads(cpus)
            .build()
            .unwrap()
    }
}
