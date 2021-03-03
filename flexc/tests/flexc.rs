use async_trait::async_trait;
use std::{
    sync::{Arc, Weak},
    time::*,
    usize,
};

// cargo test --manifest-path flexc/Cargo.toml --release -- --test-threads=1 --nocapture
#[cfg(any(feature = "tokio-rt"))]
use tokio::{
    task::{spawn, yield_now},
    test as atest,
    time::delay_for as sleep,
};

// cargo test --no-default-features --features async-rt  --manifest-path flexc/Cargo.toml --release -- --test-threads=1 --nocapture
#[cfg(any(feature = "async-rt"))]
use async_std::{
    task::{sleep, spawn, yield_now},
    test as atest,
};

use flexc::Manager;
type Pool = flexc::Pool<FakeManager>;

#[derive(Debug, Clone)]
struct FakeManager(Arc<()>);

impl FakeManager {
    fn new() -> Self {
        Self(Arc::new(()))
    }
    fn size(&self) -> usize {
        Arc::weak_count(&self.0)
    }
}

#[derive(Debug)]
struct FakeConn {
    weak: Weak<()>,
    count: usize,
}

#[async_trait]
impl Manager for FakeManager {
    type Connection = FakeConn;
    type Error = ();

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let weak = Arc::downgrade(&self.0);
        Ok(FakeConn { weak, count: 0 })
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        if conn.weak.upgrade().is_none() {
            return Err(());
        }
        Ok(())
    }
}

#[atest]
async fn test_basic() {
    let manager = FakeManager::new();
    let duration = Some(Duration::from_secs(1));
    let pool = Pool::builder()
        .maxsize(16)
        .timeout(duration)
        .check(duration)
        .build(manager.clone());

    let status = pool.state();
    assert_eq!(status.size, 0);
    assert_eq!(status.idle, 0);
    assert_eq!(manager.size(), 0);

    let con0 = pool.get().await.unwrap();
    let status = pool.state();
    assert_eq!(status.size, 1);
    assert_eq!(status.idle, 0);
    assert_eq!(manager.size(), 1);

    let con1 = pool.get().await.unwrap();
    let status = pool.state();
    assert_eq!(status.size, 2);
    assert_eq!(status.idle, 0);
    assert_eq!(manager.size(), 2);

    let con2 = pool.get().await.unwrap();
    let status = pool.state();
    assert_eq!(status.size, 3);
    assert_eq!(status.idle, 0);
    assert_eq!(manager.size(), 3);

    drop(con0);
    let status = pool.state();
    assert_eq!(status.size, 3);
    assert_eq!(status.idle, 1);
    assert_eq!(manager.size(), 3);

    drop(con1);
    let status = pool.state();
    assert_eq!(status.size, 3);
    assert_eq!(status.idle, 2);
    assert_eq!(manager.size(), 3);

    drop(con2);
    let status = pool.state();
    assert_eq!(status.size, 3);
    assert_eq!(status.idle, 3);
    assert_eq!(manager.size(), 3);
}

#[atest]
async fn test_drop() {
    let duration = Some(Duration::from_secs(1));
    let pool = Pool::builder()
        .maxsize(16)
        .timeout(duration)
        .check(duration)
        .build(FakeManager::new());

    // fetch the only conect from the pool
    let con = pool.get().await.unwrap();
    yield_now().await;
    drop(pool);
    println!("{:?}", con.as_ref());
}

#[atest]
async fn test_concurrent() {
    const TASKS: usize = 100;
    const MAX_SIZE: u32 = 3;

    let manager = FakeManager::new();
    let duration = Some(Duration::from_secs(1));
    let pool = Arc::new(
        Pool::builder()
            .maxsize(MAX_SIZE as _)
            .timeout(duration)
            .check(duration)
            .build(manager.clone()),
    );

    // Spawn tasks
    let futures = (0..TASKS)
        .map(|_| {
            let pool = pool.clone();
            spawn(async move {
                let mut con = pool.get().await.expect("get concurrent");
                con.count += 1;
                sleep(Duration::from_millis(1)).await;
            })
        })
        .collect::<Vec<_>>();

    // Await tasks to finish
    for future in futures {
        #[cfg(any(feature = "tokio-rt"))]
        future.await.unwrap();
        #[cfg(any(feature = "async-rt"))]
        future.await;
    }

    // Verify
    let status = pool.state();
    assert_eq!(status.inuse, 0);
    assert_eq!(status.size, MAX_SIZE);
    assert_eq!(status.idle, MAX_SIZE);
    assert_eq!(status.maxsize, MAX_SIZE);
    assert_eq!(manager.size(), MAX_SIZE as _);

    let values = [
        pool.get().await.unwrap(),
        pool.get().await.unwrap(),
        pool.get().await.unwrap(),
    ];

    let status = pool.state();
    assert_eq!(status.idle, 0);
    assert_eq!(status.inuse, MAX_SIZE);

    assert_eq!(
        values.iter().map(|c| c.as_ref().count).sum::<usize>(),
        TASKS
    );
}

#[atest]
async fn test_take_all() {
    take(20, 20).await
}

#[atest]
async fn test_timeout_not_block() {
    take(10, 20).await
}

// https://github.com/djc/bb8/issues/67#issuecomment-743845507
// cargo test --manifest-path flexc/Cargo.toml --release -- --test-threads=1 --nocapture
#[cfg(test)]
async fn take(maxsize: usize, takes: usize) {
    use futures::{future, Future};
    let pool = Pool::builder()
        .maxsize(maxsize)
        .timeout(Some(Duration::from_millis(100)))
        .build(FakeManager::new());

    let mut connections = Vec::new();

    // With flavor = "current_thread" this issue never occurs if the number of connections acquired
    // here is less then the pool size. With flavor = "multi_thread", however, this still
    // *sometimes* occurs as long as at least one connection is acquired.
    for _ in 0..takes {
        connections.push(pool.get().await);
    }

    let mut futures = Vec::new();

    for _ in 0..takes {
        futures.push(Box::pin(pool.get()));

        // Poll the future once.
        future::poll_fn(|context| {
            let _ = futures.last_mut().unwrap().as_mut().poll(context);
            std::task::Poll::Ready(())
        })
        .await;
    }

    // The order in which these are dropped is important, and the futures need to be dropped, not awaited.
    drop(connections);
    drop(futures);

    pool.get().await.unwrap(); // error!
}
