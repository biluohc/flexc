include!("base.rs");

const MAX_SIZE: usize = 20;
const REDIS_URL: &str = "redis://127.0.0.1:6379/";

// https://github.com/djc/bb8/issues/67#issuecomment-743845507
// cargo test --manifest-path flexc-redis/Cargo.toml --release -- --test-threads=1 --nocapture redis_pool_take_timeout
#[tokio::test]
async fn redis_pool_take_timeout() {
    use futures::{future, Future};

    let builder = Builder::default().maxsize(MAX_SIZE);
    let pool = get_redis_pool(REDIS_URL, builder).await;

    let mut connections = Vec::new();

    // With flavor = "current_thread" this issue never occurs if the number of connections acquired
    // here is less then the pool size. With flavor = "multi_thread", however, this still
    // *sometimes* occurs as long as at least one connection is acquired.
    for _ in 0..MAX_SIZE {
        connections.push(pool.get().await);
    }

    let mut futures = Vec::new();

    for _ in 0..MAX_SIZE {
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
