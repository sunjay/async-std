use crate::io::IoSliceMut;
use std::fmt;
use std::pin::Pin;

use crate::io::{self, BufRead, Read};
use crate::task::{Context, Poll};

/// Adaptor to chain together two readers.
///
/// This struct is generally created by calling [`chain`] on a reader.
/// Please see the documentation of [`chain`] for more details.
///
/// [`chain`]: trait.Read.html#method.chain
pub struct Chain<T, U> {
    pub(crate) first: T,
    pub(crate) second: U,
    pub(crate) done_first: bool,
}

impl<T, U> Chain<T, U> {
    /// Consumes the `Chain`, returning the wrapped readers.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn main() -> async_std::io::Result<()> { async_std::task::block_on(async {
    /// #
    /// use async_std::prelude::*;
    /// use async_std::fs::File;
    ///
    /// let foo_file = File::open("foo.txt").await?;
    /// let bar_file = File::open("bar.txt").await?;
    ///
    /// let chain = foo_file.chain(bar_file);
    /// let (foo_file, bar_file) = chain.into_inner();
    /// #
    /// # Ok(()) }) }
    /// ```
    pub fn into_inner(self) -> (T, U) {
        (self.first, self.second)
    }

    /// Gets references to the underlying readers in this `Chain`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn main() -> async_std::io::Result<()> { async_std::task::block_on(async {
    /// #
    /// use async_std::prelude::*;
    /// use async_std::fs::File;
    ///
    /// let foo_file = File::open("foo.txt").await?;
    /// let bar_file = File::open("bar.txt").await?;
    ///
    /// let chain = foo_file.chain(bar_file);
    /// let (foo_file, bar_file) = chain.get_ref();
    /// #
    /// # Ok(()) }) }
    /// ```
    pub fn get_ref(&self) -> (&T, &U) {
        (&self.first, &self.second)
    }

    /// Gets mutable references to the underlying readers in this `Chain`.
    ///
    /// Care should be taken to avoid modifying the internal I/O state of the
    /// underlying readers as doing so may corrupt the internal state of this
    /// `Chain`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn main() -> async_std::io::Result<()> { async_std::task::block_on(async {
    /// #
    /// use async_std::prelude::*;
    /// use async_std::fs::File;
    ///
    /// let foo_file = File::open("foo.txt").await?;
    /// let bar_file = File::open("bar.txt").await?;
    ///
    /// let mut chain = foo_file.chain(bar_file);
    /// let (foo_file, bar_file) = chain.get_mut();
    /// #
    /// # Ok(()) }) }
    /// ```
    pub fn get_mut(&mut self) -> (&mut T, &mut U) {
        (&mut self.first, &mut self.second)
    }
}

impl<T: fmt::Debug, U: fmt::Debug> fmt::Debug for Chain<T, U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chain")
            .field("t", &self.first)
            .field("u", &self.second)
            .finish()
    }
}

impl<T: Read + Unpin, U: Read + Unpin> Read for Chain<T, U> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if !self.done_first {
            let rd = Pin::new(&mut self.first);

            match futures_core::ready!(rd.poll_read(cx, buf)) {
                Ok(0) if !buf.is_empty() => self.done_first = true,
                Ok(n) => return Poll::Ready(Ok(n)),
                Err(err) => return Poll::Ready(Err(err)),
            }
        }

        let rd = Pin::new(&mut self.second);
        rd.poll_read(cx, buf)
    }

    fn poll_read_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
    ) -> Poll<io::Result<usize>> {
        if !self.done_first {
            let rd = Pin::new(&mut self.first);

            match futures_core::ready!(rd.poll_read_vectored(cx, bufs)) {
                Ok(0) if !bufs.is_empty() => self.done_first = true,
                Ok(n) => return Poll::Ready(Ok(n)),
                Err(err) => return Poll::Ready(Err(err)),
            }
        }

        let rd = Pin::new(&mut self.second);
        rd.poll_read_vectored(cx, bufs)
    }
}

impl<T: BufRead + Unpin, U: BufRead + Unpin> BufRead for Chain<T, U> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let Self {
            first,
            second,
            done_first,
        } = unsafe { self.get_unchecked_mut() };

        if !*done_first {
            let first = unsafe { Pin::new_unchecked(first) };
            match futures_core::ready!(first.poll_fill_buf(cx)) {
                Ok(buf) if buf.is_empty() => {
                    *done_first = true;
                }
                Ok(buf) => return Poll::Ready(Ok(buf)),
                Err(err) => return Poll::Ready(Err(err)),
            }
        }

        let second = unsafe { Pin::new_unchecked(second) };
        second.poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        if !self.done_first {
            let rd = Pin::new(&mut self.first);
            rd.consume(amt)
        } else {
            let rd = Pin::new(&mut self.second);
            rd.consume(amt)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::io;
    use crate::prelude::*;
    use crate::task;

    #[test]
    fn test_chain_basics() -> std::io::Result<()> {
        let source1: io::Cursor<Vec<u8>> = io::Cursor::new(vec![0, 1, 2]);
        let source2: io::Cursor<Vec<u8>> = io::Cursor::new(vec![3, 4, 5]);

        task::block_on(async move {
            let mut buffer = Vec::new();

            let mut source = source1.chain(source2);

            assert_eq!(6, source.read_to_end(&mut buffer).await?);
            assert_eq!(buffer, vec![0, 1, 2, 3, 4, 5]);

            Ok(())
        })
    }
}
