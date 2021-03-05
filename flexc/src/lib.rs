use crossbeam_queue::ArrayQueue;
use std::fmt;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use compat::{timeout, OwnedSemaphorePermit, Semaphore, SemaphoreWrap};

pub use async_trait::async_trait;
pub use error::Error;
pub use status::State;
use status::Status;

mod compat;
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

    pub fn manager(&self) -> &M {
        &self.shared.manager
    }

    pub fn config(&self) -> &Builder {
        &self.shared.cfg
    }

    /// get without waiting idle connection, default timeout is for connect and check
    pub async fn try_get(&self) -> Result<Option<PooledConnection<M>>, Error<M::Error>> {
        self.try_get_timeout(self.config().timeout).await
    }

    /// get without waiting idle connection, custom timeout is for connect and check
    pub async fn try_get_timeout(
        &self,
        duration: Option<Duration>,
    ) -> Result<Option<PooledConnection<M>>, Error<M::Error>> {
        let _wait = Arc::downgrade(&self.shared.status.0);
        let mut error = "wait";

        let permit = match self.shared.semaphore.wrapped_try_acquire_owned() {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(None),
            Err(_) => return Err(Error::Closed),
        };

        let conn = self.shared.queue.pop();
        if conn.is_none() {
            return Ok(None);
        }

        let fut = async move {
            let mut conn = PooledConnection(conn);
            let con = conn.0.as_mut().expect("try get");
            con.permit = Some(permit);

            match self.fill_conn(&mut error, con).await {
                Ok(()) => {
                    con.inuse();
                    Ok(Some(conn))
                }
                Err(e) => {
                    con.recycle();
                    Err(e)
                }
            }
        };

        if let Some(duration) = duration {
            match timeout(duration, fut).await {
                Ok(res) => res,
                Err(_) => Err(Error::Timeout(error)),
            }
        } else {
            fut.await
        }
    }

    /// get with default timeout
    pub async fn get(&self) -> Result<PooledConnection<M>, Error<M::Error>> {
        self.get_timeout(self.config().timeout).await
    }

    /// get with custom timeout
    pub async fn get_timeout(
        &self,
        duration: Option<Duration>,
    ) -> Result<PooledConnection<M>, Error<M::Error>> {
        let _wait = Arc::downgrade(&self.shared.status.0);
        let mut error = "wait";

        if let Some(duration) = duration {
            match timeout(duration, self.get_inner(&mut error)).await {
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
                match self.shared.semaphore.wrapped_try_acquire_owned() {
                    Ok(Some(p)) => p,
                    Ok(None) => continue,
                    Err(_) => return Err(Error::Closed),
                }
            } else {
                self.shared
                    .semaphore
                    .wrapped_acquire_owned()
                    .await
                    .map_err(|_| Error::Closed)?
            };

            let conn = self.shared.queue.pop();
            if conn.is_none() {
                continue;
            }

            let mut conn = PooledConnection(conn);
            let con = conn.0.as_mut().expect("get");
            con.permit = Some(permit);

            return match self.fill_conn(error, con).await {
                Ok(()) => {
                    con.inuse();
                    Ok(conn)
                }
                Err(e) => {
                    con.recycle();
                    Err(e)
                }
            };
        }
    }

    async fn fill_conn(
        &self,
        error: &mut &'static str,
        conn: &mut Conn<M>,
    ) -> Result<(), Error<M::Error>> {
        let new = conn.is_empty();
        if new {
            *error = "connect";
            let con = self.manager().connect().await?;
            conn.con = Some(con);
        }

        // todo: should drop _wait while check?
        if let Some(check) = self.config().check {
            if new
                || check == Duration::from_secs(0)
                || self.shared.clock.elapsed() >= (conn.time + check)
            {
                *error = "check";
                self.manager().check(conn.con.as_mut().unwrap()).await?;
                conn.time = self.shared.clock.elapsed();
            }
        }

        Ok(())
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
            check: Some(Duration::from_secs(0)),
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
    None => never check

    0 => check every time

    >0 => check every duration
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
    semaphore: Arc<Semaphore>,
    queue: ArrayQueue<Conn<M>>,
    status: Status,
    clock: Instant,
}

impl<M: Manager> SharedPool<M> {
    pub(crate) fn new(cfg: Builder, manager: M) -> Self {
        let semaphore = Semaphore::wrapped_new(cfg.maxsize);
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
    type Error: Send + 'static;

    /// Attempts to create a new connection.
    async fn connect(&self) -> Result<Self::Connection, Self::Error>;

    /// Determines if the connection is still connected to the database when check-out.
    ///
    /// A standard implementation would check if a simple query like `SELECT 1`
    /// succeeds.
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error>;
}
/// A smart pointer wrapping a connection.
#[derive(Debug)]
pub struct PooledConnection<M: Manager>(Option<Conn<M>>);

pub(crate) struct Conn<M: Manager> {
    idx: usize,
    time: Duration,
    status: Status,
    shared: Weak<SharedPool<M>>,
    con: Option<M::Connection>,
    permit: Option<OwnedSemaphorePermit>,
}

impl<M: Manager> fmt::Debug for Conn<M> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Conn")
            .field("idx", &self.idx)
            .field("time", &self.time)
            .field("state", &self.status.0[self.idx])
            .field("con", &self.con.as_ref().map(|_| ()))
            .field("permit", &self.con.as_ref().map(|_| ()))
            .field("shared", &"..")
            .finish()
    }
}

impl<M: Manager> Conn<M> {
    pub(crate) fn new(idx: usize, shared: &Arc<SharedPool<M>>) -> Self {
        Self {
            idx,
            shared: Arc::downgrade(shared),
            status: shared.status.clone(),
            time: Duration::from_secs(0),
            con: None,
            permit: None,
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.con.is_none()
    }
    pub(crate) fn inuse(&mut self) {
        self.status.set_inuse(self.idx);
    }
    // recycle when connect/check error
    pub(crate) fn recycle(&mut self) {
        self.con.take();
        self.status.set_empty(self.idx);
    }
}

impl<M: Manager> PooledConnection<M> {
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
        let mut conn = self.0.take().unwrap();
        if conn.is_empty() {
            conn.status.set_empty(conn.idx);
        } else {
            conn.status.set_idle(conn.idx);
        }

        // the pool not dropped
        if let Some(p) = conn.shared.upgrade() {
            conn.permit.take();
            p.queue.push(conn).ok();
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
