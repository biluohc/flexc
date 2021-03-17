use std::{
    sync::{atomic::*, Arc, Weak},
    time::*,
};

// cargo test --manifest-path flexc/Cargo.toml --release -- --test-threads=1 --nocapture
#[cfg(any(feature = "tokio-rt"))]
use tokio::{
    task::{spawn, yield_now},
    test as atest,
    time::sleep,
};

// cargo test --no-default-features --features async-rt  --manifest-path flexc/Cargo.toml --release -- --test-threads=1 --nocapture
#[cfg(any(feature = "async-rt"))]
use async_std::{
    task::{sleep, spawn, yield_now},
    test as atest,
};

use flexc::{async_trait, Manager};
type Pool = flexc::Pool<MockManager>;

#[derive(Debug, Clone)]
struct MockManager {
    rc: Arc<AtomicUsize>,
    bad: bool,
    clock: Instant,
}

impl MockManager {
    fn new() -> Self {
        Self::with_bad(false)
    }
    fn with_bad(bad: bool) -> Self {
        Self {
            clock: Instant::now(),
            rc: Arc::new(AtomicUsize::new(0)),
            bad,
        }
    }
    fn size(&self) -> usize {
        Arc::weak_count(&self.rc)
    }
    fn connect_costed(&self) -> Duration {
        Duration::from_millis(2)
    }
    fn check_costed(&self) -> Duration {
        Duration::from_millis(10)
    }
    fn bad_range(&self) -> (usize, usize) {
        (10, 20)
    }
    fn count(&self) -> usize {
        self.rc.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Debug)]
struct MockConn {
    weak: Weak<AtomicUsize>,
    count: usize,
    checked_times: usize,
    connect_time: Duration,
    connected_time: Option<Duration>,
    check_time: Option<Duration>,
    checked_time: Option<Duration>,
}

#[async_trait]
impl Manager for MockManager {
    type Connection = MockConn;
    type Error = ();

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let connect_time = self.clock.elapsed();
        // mock cost
        sleep(self.connect_costed()).await;

        let weak = Arc::downgrade(&self.rc);
        Ok(MockConn {
            weak,
            count: 0,
            connect_time,
            checked_times: 0,
            check_time: None,
            checked_time: None,
            connected_time: Some(self.clock.elapsed()),
        })
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        if conn.weak.upgrade().is_none() {
            return Err(());
        }
        conn.check_time = Some(self.clock.elapsed());
        // mock cost
        sleep(self.check_costed()).await;

        // return error for bad
        if self.bad {
            let range = self.bad_range();
            let count = self.count();
            if count >= range.0 && count < range.1 {
                return Err(());
            }
        }

        conn.checked_time = Some(self.clock.elapsed());
        conn.checked_times += 1;
        Ok(())
    }
}

#[atest]
async fn test_basic() {
    let manager = MockManager::new();
    let duration = Some(Duration::from_secs(1));
    let pool = Pool::builder()
        .maxsize(16)
        .timeout(duration)
        .check(duration)
        .build_unchecked(manager.clone());

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
async fn test_move_drop() {
    let duration = Some(Duration::from_secs(1));
    let pool = Pool::builder()
        .maxsize(16)
        .timeout(duration)
        .check(duration)
        .build_unchecked(MockManager::new());

    // fetch the only conect from the pool
    let con = pool.get().await.unwrap();
    let con2 = pool.get().await.unwrap();
    let con3 = pool.try_get().await.unwrap().unwrap();

    yield_now().await;

    spawn(async move { con3 });

    std::thread::spawn(move || {
        drop(pool);
        drop(con2);
    })
    .join()
    .expect("pool move to new thread and drop");

    println!("{:?}", con.as_ref());
}

#[atest]
async fn test_concurrent() {
    const TASKS: usize = 100;
    const MAX_SIZE: u32 = 3;

    let manager = MockManager::new();
    let duration = Some(Duration::from_secs(2));
    let pool = Arc::new(
        Pool::builder()
            .maxsize(MAX_SIZE as _)
            .timeout(duration)
            .build_unchecked(manager.clone()),
    );

    // Init
    pool.start_connections().await.unwrap();
    let status = pool.state();
    assert_eq!(status.inuse, 0);
    assert_eq!(status.size, MAX_SIZE);
    assert_eq!(status.idle, MAX_SIZE);
    assert_eq!(status.maxsize, MAX_SIZE);
    assert_eq!(manager.size(), MAX_SIZE as _);

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
    assert_eq!(
        values
            .iter()
            .map(|c| c.as_ref().checked_times)
            .sum::<usize>(),
        TASKS + values.len() * 2
    );

    let now = manager.clock.elapsed();
    values.iter().for_each(|c| {
        assert!(c.as_ref().connect_time < c.as_ref().connected_time.unwrap());

        let ctime = c.as_ref().checked_time.unwrap();
        assert!(
            ctime < now,
            "conn.checked_time {:?} < manager.time {:?}",
            c.as_ref().checked_time,
            now
        );

        let ctime = c.as_ref().check_time.unwrap();
        assert!(
            now - ctime > manager.check_costed(),
            "manager.time - conn.check_time  {:?} - {:?} < manager.check_costed() {:?}",
            now,
            ctime,
            manager.check_costed(),
        );
    })
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
        .build(MockManager::new())
        .await
        .unwrap();

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

#[atest]
async fn test_bad_check() {
    const GETS: usize = 100;
    const MAX_SIZE: u32 = 12;

    let manager = MockManager::with_bad(true);
    let duration = Some(Duration::from_secs(1));
    let pool = Pool::builder()
        .maxsize(MAX_SIZE as _)
        .timeout(duration)
        .build_unchecked(manager.clone());

    let mut errc = 0usize;
    let mut okc = 0usize;
    let mut cons = vec![];
    let (bad_start, bad_end) = manager.bad_range();

    for _ in 0..bad_start {
        cons.push(pool.get().await.unwrap());
        okc += 1;
    }

    for _ in bad_start..bad_end {
        assert!(pool.get().await.unwrap_err().is_inner());
        errc += 1;
    }

    for _ in bad_end..GETS {
        pool.get().await.unwrap();
        okc += 1;
    }

    assert_eq!(okc, GETS - bad_end + bad_start);
    assert_eq!(errc, bad_end - bad_start);

    cons.drain(..).into_iter().map(|c| c.take()).count();
    for _ in 0..MAX_SIZE {
        cons.push(pool.get().await.unwrap());
    }
    assert_eq!(cons.len() as u32, MAX_SIZE);
    let state = pool.state();
    assert_eq!(state.maxsize, MAX_SIZE);
    assert_eq!(state.size, MAX_SIZE);
    assert_eq!(state.inuse, MAX_SIZE);
    assert_eq!(state.idle, 0);
    assert!(pool.get().await.unwrap_err().is_timeout());
    assert!(pool.try_get().await.unwrap().is_none());
}
