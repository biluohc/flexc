include!("base.rs");

const REDIS_URL: &str = "redis://127.0.0.1:6379/";
const KEY: &str = "set-take-idxs";

// cargo test --manifest-path flexc-redis/Cargo.toml --release -- --test-threads=1 --nocapture redis_pool_take_conns
// cargo test --no-default-features --features async-rt --manifest-path flexc-redis/Cargo.toml --release -- --test-threads=1 --nocapture redis_pool_take_conns
#[test]
fn redis_pool_take_conns() {
    block_on(async {
        let builder = Builder::default()
            .maxsize(7)
            .timeout(Some(Duration::from_secs(1)));
        let pool = get_redis_pool(REDIS_URL, builder).await;
        {
            let mut conn = pool.get().await.unwrap();
            let mut query = redis::Cmd::new();
            query.arg("del").arg(KEY);
            let _: () = query.query_async(conn.as_mut()).await.unwrap();
        }

        let (mp, mc) = mpmc::unbounded();
        let counter = Counter::new();
        let tasks = num_cpus::get() * num_cpus::get() * 3;

        for idx in 0..tasks {
            let cc = counter.clone();
            let p = pool.clone();
            let mp = mp.clone();
            spawn(async move {
                while cc.count() < 1 {
                    sleep(Duration::from_millis(1)).await;
                }
                // let now = Instant::now();
                let mut pc = p.get().await.unwrap();
                let rest: i32 = redis::Cmd::sadd(KEY, idx as i32)
                    .query_async(&mut *pc)
                    .await
                    .unwrap();
                // println!(
                //     "{} {:?}: SADD({}, {}), {:?}",
                //     idx,
                //     now.elapsed(),
                //     KEY,
                //     idx,
                //     rest
                // );
                assert!(rest > 0);
                mp.send(idx).unwrap();
            });
        }
        std::mem::drop(mp);
        counter.counter();
        let count = mc.into_iter().count();
        println!("tasks: {}, mp.count: {}", tasks, count);
        assert_eq!(tasks, count)
    })
}
