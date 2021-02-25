extern crate flexc;

// use async_std::task;
use flexc::redis::{Pool, RedisConnectionManager};
use redis::AsyncCommands;
use std::sync::Arc;
use std::time::Instant;
use tokio::task;

const TEST_KEY: &'static str = "mobc::redis::test";
const REDIS_URL: &str = "redis://127.0.0.1:6379/";

#[tokio::main]
async fn main() {
    let client = redis::Client::open(REDIS_URL).unwrap();
    let manager = RedisConnectionManager::new(client);
    let pool = Arc::new(Pool::builder().cap(20).build(manager));
    println!("state: {:?}", pool.state());

    let mut conn = pool.get().await.unwrap();
    println!("state: {:?}", pool.state());
    let _: () = conn.set(TEST_KEY, "hello").await.unwrap();
    println!("state: {:?}", pool.state());

    const MAX: usize = 5000;
    let now = Instant::now();
    let (tx, rx) = async_channel::bounded::<usize>(5000);
    for i in 0..MAX {
        let pool = pool.clone();
        let tx_c = tx.clone();
        task::spawn(async move {
            let mut conn = pool.get().await.unwrap();
            let s: String = conn.get(TEST_KEY).await.unwrap();
            assert_eq!(s.as_str(), "hello");
            tx_c.send(i).await.unwrap();
        });
    }
    for _ in 0..MAX {
        rx.recv().await.unwrap();
    }

    println!("state: {:?}, cost: {:?}", pool.state(), now.elapsed());
    conn.take();
    println!("state: {:?}, cost: {:?}", pool.state(), now.elapsed());
}
