use crate::{async_trait, Manager};
use redis::aio::Connection;
use redis::{Client, ErrorKind, RedisError};
use std::sync::atomic::*;

pub type Pool = crate::Pool<RedisConnectionManager>;
pub type PooledConnection = crate::PooledConnection<RedisConnectionManager>;
pub type Error = crate::Error<RedisError>;

pub struct RedisConnectionManager {
    client: Client,
    counter: AtomicUsize,
}

impl RedisConnectionManager {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            counter: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Manager for RedisConnectionManager {
    type Connection = Connection;
    type Error = RedisError;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let c = self.client.get_async_connection().await?;
        Ok(c)
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        let count = self.counter.fetch_add(1, Ordering::Relaxed) % 1000000;
        let pong: usize = redis::cmd("PING").arg(count).query_async(conn).await?;
        if pong != count {
            return Err((ErrorKind::ResponseError, "pong response error").into());
        }
        Ok(())
    }
}
