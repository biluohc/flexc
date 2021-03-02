include!("base.rs");

const REDIS_URL: &str = "redis://127.0.0.1:6379/";
const KEY: &str = "set-bench";

// while true; do sleep 1&& redis-cli info stats|grep per_sec; done
// cargo test --manifest-path flexc-redis/Cargo.toml --release -- --test-threads=1 --nocapture redis_pool_bench
#[test]
fn redis_pool_bench() {
    let mut rt = tokio_rt(num_cpus::get() * 5);

    rt.block_on(async {
        let builder = Builder::default()
            .maxsize(7)
            .timeout(Some(Duration::from_secs(1)));
        let pool = get_redis_pool(REDIS_URL, builder).await;
        let counter = Counter::new();
        let time = Duration::from_secs(60);

        for idx in 0..num_cpus::get() * num_cpus::get() * 3 {
            let pool = pool.clone();
            let cc = counter.clone();
            let time = time.clone();

            spawn(async move {
                let set = KEY;

                while cc.count() < 1 {
                    sleep(Duration::from_millis(1)).await;
                }
                let now = Instant::now();

                while now.elapsed() < time {
                    let mut con = pool.get().await.expect("get conn");
                    if now.elapsed().as_micros() % 1000 == 0 {
                        let mut pl = redis::Pipeline::new();
                        pl.cmd("sadd")
                            .arg(set)
                            .arg(idx)
                            .cmd("expire")
                            .arg(set)
                            .arg(300);

                        let rest: Result<(i32, i32), _> = pl.query_async(con.as_mut()).await;
                        assert!(rest.is_ok());
                    } else {
                        let mut cmd = redis::Cmd::new();
                        cmd.arg("SADD").arg(set).arg(idx);

                        let rest: Result<i32, _> = cmd.query_async(con.as_mut()).await;
                        assert!(rest.is_ok(), "{:?}", rest);
                    }
                    cc.counter();
                }
                // println!("redisc-bench-{} exit", idx);
            });
        }

        counter.counter();
        sleep(time.clone() + Duration::from_secs(1)).await;
        let count = counter.count() - 1;

        println!(
            "costed: {:?}, count: {}, ops: {}",
            time,
            count,
            count / time.as_secs() as usize
        );
    })
}
