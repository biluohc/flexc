use std::{error, fmt};
/// The error type returned by methods in this crate.
pub enum Error<E> {
    /// Manager Errors
    Inner(E),
    /// Timeout
    Timeout(&'static str),
    /// Pool already closed
    Closed,
}

impl<E> From<E> for Error<E> {
    fn from(e: E) -> Error<E> {
        Error::Inner(e)
    }
}

impl<E> fmt::Display for Error<E>
where
    E: fmt::Display + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Inner(ref err) => write!(f, "{}", err),
            Error::Timeout(loc) => write!(f, "Timed out in flexc.{}", loc),
            Error::Closed => write!(f, "Pool Closed in flexc"),
        }
    }
}

impl<E> fmt::Debug for Error<E>
where
    E: fmt::Debug + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Inner(ref err) => write!(f, "{:?}", err),
            Error::Timeout(loc) => write!(f, "Timed out in flexc.{}", loc),
            Error::Closed => write!(f, "Pool Closed in flexc"),
        }
    }
}

impl<E> error::Error for Error<E>
where
    E: error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::Inner(ref err) => Some(err),
            _ => None,
        }
    }
}
