extern crate flexc;

use flexc::redis::{Pool, RedisConnectionManager};
use redis::AsyncCommands;
use std::sync::Arc;
use std::time::Instant;

const TEST_KEY: &'static str = "mobc::redis::test";

#[tokio::main]
async fn main() {
    let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let manager = RedisConnectionManager::new(client);
    let pool = Arc::new(Pool::builder().cap(20).build(manager));

    let mut conn = pool.get().await.unwrap();
    let _: () = conn.set(TEST_KEY, "hello").await.unwrap();

    const MAX: usize = 5000;

    let now = Instant::now();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<usize>(5000);
    for i in 0..MAX {
        let pool = pool.clone();
        let mut tx_c = tx.clone();
        tokio::spawn(async move {
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
