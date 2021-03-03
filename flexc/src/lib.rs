use crossbeam_queue::ArrayQueue;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

#[cfg(any(feature = "tokio-rt"))]
use tokio::{sync::Semaphore, time};

pub use async_trait::async_trait;
pub use error::Error;
pub use status::State;
use status::Status;

mod error;
mod status;

pub struct Pool<M: Manager> {
    shared: Arc<SharedPool<M>>,
}

impl<M: Manager> Pool<M> {
    pub fn new(manager: M) -> Self {
        let cfg = Self::builder();
        cfg.build(manager)
    }

    pub fn builder() -> Builder {
        Builder::default()
    }

    pub fn state(&self) -> State {
        self.shared.status.state()
    }

    pub async fn get(&self) -> Result<PooledConnection<M>, Error<M::Error>> {
        let _wait = Arc::downgrade(&self.shared.status.0);
        let mut error = "wait";

        if let Some(timeout) = self.shared.cfg.timeout {
            match time::timeout(timeout, self.get_inner(&mut error)).await {
                Ok(res) => res,
                Err(_) => Err(Error::Timeout(error)),
            }
        } else {
            self.get_inner(&mut error).await
        }
    }

    async fn get_inner(
        &self,
        error: &mut &'static str,
    ) -> Result<PooledConnection<M>, Error<M::Error>> {
        let mut try_once_time = true;

        loop {
            let permit = if try_once_time {
                try_once_time = false;
                match self.shared.semaphore.try_acquire() {
                    Ok(p) => p,
                    Err(_) => continue,
                }
            } else {
                self.shared.semaphore.acquire().await
            };
            let conn = self.shared.queue.pop();
            permit.forget();
            if conn.is_none() {
                continue;
            }

            let mut conn = conn.unwrap();
            if conn.is_empty() {
                *error = "connect";
                let con = self.shared.manager.connect().await?;
                conn.con = Some(con);
            }

            // todo: incheck should drop _wait?
            if let Some(check) = self.shared.cfg.check {
                if check == Duration::from_secs(0)
                    || self.shared.clock.elapsed() >= (conn.time + check)
                {
                    *error = "check";
                    self.shared
                        .manager
                        .check(conn.con.as_mut().unwrap())
                        .await?;
                    conn.time = self.shared.clock.elapsed();
                }
            }

            return Ok(PooledConnection::new(conn));
        }
    }
}

impl<M: Manager> Drop for Pool<M> {
    fn drop(&mut self) {}
}
#[derive(Clone, Debug)]
pub struct Builder {
    maxsize: usize,
    check: Option<Duration>,
    timeout: Option<Duration>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            maxsize: 20,
            timeout: Some(Duration::from_secs(5)),
            check: Some(Duration::from_secs(10)),
        }
    }
}

impl Builder {
    pub fn maxsize(mut self, maxsize: usize) -> Self {
        assert!(maxsize > 0);
        self.maxsize = maxsize;
        self
    }
    /*
    none => never check

    0 => check every times

    >0 => check every time
    */
    pub fn check(mut self, check_duration: Option<Duration>) -> Self {
        self.check = check_duration;
        self
    }
    /// get connection from pool timeout
    pub fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn build<M: Manager>(self, manager: M) -> Pool<M> {
        let shared = Arc::new(SharedPool::new(self, manager));

        for idx in 0..shared.cfg.maxsize {
            let conn = Conn::new(idx, &shared);
            shared.queue.push(conn).ok();
        }

        Pool { shared }
    }
}

pub(crate) struct SharedPool<M: Manager> {
    cfg: Builder,
    manager: M,
    semaphore: Semaphore,
    queue: ArrayQueue<Conn<M>>,
    status: Status,
    clock: Instant,
}

impl<M: Manager> SharedPool<M> {
    pub(crate) fn new(cfg: Builder, manager: M) -> Self {
        let semaphore = Semaphore::new(cfg.maxsize);
        let queue = ArrayQueue::new(cfg.maxsize);
        let status = Status::new(cfg.maxsize);
        Self {
            cfg,
            manager,
            status,
            queue,
            semaphore,
            clock: Instant::now(),
        }
    }
}

#[async_trait]
/// A trait which provides connection-specific functionality.
pub trait Manager: Send + Sync + 'static {
    /// The connection type this manager deals with.
    type Connection: Send + 'static;
    /// The error type returned by `Connection`s.
    type Error: Send + Sync + 'static;

    /// Attempts to create a new connection.
    async fn connect(&self) -> Result<Self::Connection, Self::Error>;

    /// Determines if the connection is still connected to the database when check-out.
    ///
    /// A standard implementation would check if a simple query like `SELECT 1`
    /// succeeds.
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error>;
}
/// A smart pointer wrapping a connection.
pub struct PooledConnection<M: Manager>(Option<Conn<M>>);

pub(crate) struct Conn<M: Manager> {
    idx: usize,
    time: Duration,
    status: Status,
    shared: Weak<SharedPool<M>>,
    con: Option<M::Connection>,
}

impl<M: Manager> Conn<M> {
    pub(crate) fn new(idx: usize, shared: &Arc<SharedPool<M>>) -> Self {
        Self {
            idx,
            shared: Arc::downgrade(shared),
            status: shared.status.clone(),
            time: Duration::from_secs(0),
            con: None,
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.con.is_none()
    }
}

impl<M: Manager> PooledConnection<M> {
    pub(crate) fn new(conn: Conn<M>) -> Self {
        conn.status.set_inuse(conn.idx);
        Self(Some(conn))
    }
    /// Take this connection from the pool permanently.
    pub fn take(mut self) -> M::Connection {
        self.0.as_mut().unwrap().con.take().unwrap()
    }
}

impl<M: Manager> AsRef<M::Connection> for PooledConnection<M> {
    fn as_ref(&self) -> &M::Connection {
        self.0.as_ref().unwrap().con.as_ref().unwrap()
    }
}

impl<M: Manager> AsMut<M::Connection> for PooledConnection<M> {
    fn as_mut(&mut self) -> &mut M::Connection {
        self.0.as_mut().unwrap().con.as_mut().unwrap()
    }
}

impl<M: Manager> std::ops::Deref for PooledConnection<M> {
    type Target = M::Connection;
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<M: Manager> std::ops::DerefMut for PooledConnection<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<M: Manager> Drop for PooledConnection<M> {
    fn drop(&mut self) {
        let conn = self.0.take().unwrap();
        if conn.is_empty() {
            conn.status.set_empty(conn.idx);
        } else {
            conn.status.set_idle(conn.idx);
        }

        // the pool not dropped
        if let Some(p) = conn.shared.upgrade() {
            p.queue.push(conn).ok();
            p.semaphore.add_permits(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}