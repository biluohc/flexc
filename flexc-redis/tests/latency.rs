include!("base.rs");

const REDIS_URL: &str = "redis://127.0.0.1:6379/";
const KEY: &str = "set-lantency";
const TIME: Duration = Duration::from_secs(60);

// while true; do sleep 1&& redis-cli info stats|grep per_sec; done
// cargo test --manifest-path flexc-redis/Cargo.toml --release -- --test-threads=1 --nocapture redis_pool_lantency
// cargo test --no-default-features --features async-rt --manifest-path flexc-redis/Cargo.toml --release -- --test-threads=1 --nocapture redis_pool_lantency
#[test]
fn redis_pool_lantency() {
    block_on(async {
        let builder = Builder::default()
            .maxsize(7)
            .timeout(Some(Duration::from_secs(1)));
        let pool = get_redis_pool(REDIS_URL, builder).await;

        let counter = Counter::new();
        let counter_get_failed = Counter::new();

        let (mp, mc) = mpmc::unbounded();

        for idx in 0..num_cpus::get() * num_cpus::get() * 3 {
            let mp = mp.clone();
            let cc = counter.clone();
            let cget = counter_get_failed.clone();
            let pool = pool.clone();

            spawn(async move {
                let set = KEY;

                while cc.count() < 1 {
                    sleep(Duration::from_millis(1)).await;
                }
                let now = Instant::now();

                while now.elapsed() < TIME {
                    let mut con = match pool.get().await {
                        Ok(c) => c,
                        Err(e) => {
                            println!("get conn failed: {}", e);
                            cget.counter();
                            continue;
                        }
                    };

                    if now.elapsed().as_micros() % 1000 == 0 {
                        let mut pl = redis::Pipeline::new();
                        pl.cmd("sadd")
                            .arg(set)
                            .arg(idx)
                            .cmd("expire")
                            .arg(set)
                            .arg(300);

                        let now = Instant::now();
                        let rest: Result<(i32, i32), _> = pl.query_async(con.as_mut()).await;
                        assert!(rest.is_ok());
                        mp.send(now.elapsed().as_millis() as usize).unwrap();
                    } else {
                        let mut cmd = redis::Cmd::new();
                        cmd.arg("SADD").arg(set).arg(idx);

                        let now = Instant::now();
                        let rest: Result<i32, _> = cmd.query_async(con.as_mut()).await;
                        assert!(rest.is_ok(), "{:?}", rest);
                        mp.send(now.elapsed().as_millis() as usize).unwrap();
                    }
                    cc.counter();
                }
            });
        }

        let num = 10_000 + 1;
        let mut mss = (0..num).into_iter().map(|_| 0).collect::<Vec<u64>>();
        counter.counter();

        std::mem::drop(mp);

        for i in mc.into_iter() {
            if i > num {
                eprintln!("costed {} ms > {}", i, num);
            } else {
                mss[i] += 1;
            }
        }

        let count = counter.count() - 1;
        println!("count: {}", count);
        for (i, c) in mss
            .iter()
            .enumerate()
            .map(|(i, c)| if *c == 0 { None } else { Some((i, c)) })
            .filter_map(|ic| ic)
        {
            println!("{}: {:.2}, {}", i, *c as f64 / count as f64, c);
        }

        println!(
            "costed: {:?}, get_connection_failed: {}, count: {}, ops: {}",
            TIME,
            counter_get_failed.count(),
            count,
            count / TIME.as_secs() as usize
        );
    })
}
