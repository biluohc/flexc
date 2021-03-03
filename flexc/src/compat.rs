use std::sync::Arc;
#[derive(Debug)]
pub(crate) struct Closed;
#[crate::async_trait]
pub(crate) trait SemaphoreWrap {
    fn wrapped_new(permits: usize) -> Arc<Self>;
    fn wrapped_try_acquire_owned(self: &Arc<Self>) -> Result<Option<OwnedSemaphorePermit>, Closed>;
    async fn wrapped_acquire_owned(self: &Arc<Self>) -> Result<OwnedSemaphorePermit, Closed>;
}

#[cfg(any(feature = "tokio-rt"))]
pub(crate) use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::timeout,
};

#[cfg(any(feature = "tokio-rt"))]
#[crate::async_trait]
impl SemaphoreWrap for Semaphore {
    fn wrapped_new(permits: usize) -> Arc<Self> {
        Arc::new(Semaphore::new(permits))
    }
    fn wrapped_try_acquire_owned(self: &Arc<Self>) -> Result<Option<OwnedSemaphorePermit>, Closed> {
        Ok(self.clone().try_acquire_owned().ok())
    }
    async fn wrapped_acquire_owned(self: &Arc<Self>) -> Result<OwnedSemaphorePermit, Closed> {
        Ok(self.clone().acquire_owned().await)
    }
}

#[cfg(any(feature = "async-rt"))]
pub(crate) use async_lock::{Semaphore, SemaphoreGuardArc as OwnedSemaphorePermit};
#[cfg(any(feature = "async-rt"))]
pub(crate) use async_std::future::timeout;

#[cfg(any(feature = "async-rt"))]
#[crate::async_trait]
impl SemaphoreWrap for Semaphore {
    fn wrapped_new(permits: usize) -> Arc<Self> {
        Arc::new(Semaphore::new(permits))
    }
    fn wrapped_try_acquire_owned(self: &Arc<Self>) -> Result<Option<OwnedSemaphorePermit>, Closed> {
        Ok(self.try_acquire_arc())
    }
    async fn wrapped_acquire_owned(self: &Arc<Self>) -> Result<OwnedSemaphorePermit, Closed> {
        Ok(self.acquire_arc().await)
    }
}
