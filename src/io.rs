use bytes::Bytes;
use futures_lite::io::AsyncRead;
use futures_util::Stream;
use std::{
    io::{Error, ErrorKind},
    pin::pin,
    task::Poll,
};

const BUF_SIZE: usize = 8192;

/// Make any AsynRead implementation work as Stream
pub struct ReaderStream<R> {
    reader: R,
}

impl<R> ReaderStream<R>
where
    R: AsyncRead,
{
    /// Create a new ReaderStream
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Stream for ReaderStream<R>
where
    R: AsyncRead + Unpin,
{
    type Item = Result<Bytes, Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut buf = vec![0u8; BUF_SIZE];
        match pin!(&mut self.reader).poll_read(cx, &mut buf) {
            Poll::Ready(Ok(n)) => {
                if n == 0 {
                    return Poll::Ready(None::<Self::Item>);
                }
                buf.truncate(n);
                Poll::Ready(Some(Ok(buf.into())))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(Error::new(ErrorKind::Other, e)))),
            Poll::Pending => Poll::Pending,
        }
    }
}
