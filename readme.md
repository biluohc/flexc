# flexc
A generic asynchronous connection pool

## Features

* Support async/.await syntax
* Support both `tokio` and `async-std/smol`
* High performance
* Easy to use

## Usage

```toml
[dependencies]
# default-feature is tokio-rt
flexc = { version = "x", git = "https://github.com/biluohc/flexc" }

# For async-std runtime
# flexc = { version = "x", git = "https://github.com/biluohc/flexc", default-features = false, features = ["async-rt"] }
```

### Other projects
1. [mobc](https://github.com/importcjj/mobc): An asynchronous connection pool and rich features buts slightly high latenncy.
1. [deadpool](https://github.com/bikeshedder/deadpool): asynchronous and high performance buts Builder is very troublesome.
1. [bb8](https://github.com/djc/bb8): An asynchronous connection pool provides the same configuration options as r2d2.
2. [r2d2](https://github.com/sfackler/r2d2): A synchronized connection pool, not recommended.
1. [redis-rs](https://github.com/mitsuhiko/redis-rs): The most used Redis client, flexc-redis based on it.
3. [redis-async-rs](https://github.com/benashford/redis-async-rs): Another Redis client.
