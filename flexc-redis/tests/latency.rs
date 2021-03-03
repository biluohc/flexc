include!("base.rs");

const REDIS_URL: &str = "redis://127.0.0.1:6379/";
const KEY: &str = "set-lantency";

// while true; do sleep 1&& redis-cli info stats|grep per_sec; done
// cargo test --manifest-path flexc-redis/Cargo.toml --release -- --test-threads=1 --nocapture redis_pool_lantency
#[test]
fn redis_pool_lantency() {
    let mut rt = tokio_rt(num_cpus::get() * 5);

    rt.block_on(async {
        let builder = Builder::default()
            .maxsize(7)
            .timeout(Some(Duration::from_secs(1)));
        let pool = get_redis_pool(REDIS_URL, builder).await;
        let counter = Counter::new();
        let time = Duration::from_secs(60);
        let (mp, mc) = mpmc::unbounded();
        for idx in 0..num_cpus::get() * num_cpus::get() * 3 {
            let mp = mp.clone();
            let cc = counter.clone();
            let pool = pool.clone();
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
                // println!("redisc-bench-{} exit", idx);
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
            "costed: {:?}, count: {}, ops: {}",
            time,
            count,
            count / time.as_secs() as usize
        );
    })
}
