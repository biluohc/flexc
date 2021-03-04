use std::{
    error,
    fmt::{self, Debug, Display},
};
/// The error type returned by methods in this crate.
pub enum Error<E> {
    /// Manager Errors
    Inner(E),
    /// Timeout
    Timeout(&'static str),
    /// Pool already closed
    Closed,
}

impl<E> Error<E> {
    pub fn into_inner(self) -> Option<E> {
        match self {
            Error::Inner(e) => Some(e),
            _ => None,
        }
    }
    pub fn is_inner(&self) -> bool {
        match *self {
            Error::Inner(_) => true,
            _ => false,
        }
    }
    pub fn is_timeout(&self) -> bool {
        match *self {
            Error::Timeout(_) => true,
            _ => false,
        }
    }
    pub fn is_closed(&self) -> bool {
        match *self {
            Error::Closed => true,
            _ => false,
        }
    }
}

impl<E> From<E> for Error<E> {
    fn from(e: E) -> Error<E> {
        Error::Inner(e)
    }
}

impl<E: Display> Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Inner(ref err) => write!(f, "{}", err),
            Error::Timeout(loc) => write!(f, "Timed out in flexc.{}", loc),
            Error::Closed => write!(f, "Pool Closed in flexc"),
        }
    }
}

impl<E: Debug> Debug for Error<E> {
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
