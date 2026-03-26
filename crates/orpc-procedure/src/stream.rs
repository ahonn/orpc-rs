use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use pin_project_lite::pin_project;

use crate::error::ProcedureError;
use crate::output::DynOutput;

/// Type-erased async stream of procedure results.
///
/// Unifies single-value responses (queries, mutations) and streaming responses
/// (subscriptions) behind a common `Stream` interface.
pub struct ProcedureStream {
    inner: Pin<Box<dyn Stream<Item = Result<DynOutput, ProcedureError>> + Send>>,
}

impl ProcedureStream {
    /// Create from an existing stream.
    pub fn from_stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<DynOutput, ProcedureError>> + Send + 'static,
    {
        ProcedureStream {
            inner: Box::pin(stream),
        }
    }

    /// Create from a single-value future (for queries and mutations).
    pub fn from_future<F>(future: F) -> Self
    where
        F: Future<Output = Result<DynOutput, ProcedureError>> + Send + 'static,
    {
        ProcedureStream {
            inner: Box::pin(FutureStream::Pending { future }),
        }
    }

    /// Create an error stream that yields a single error.
    pub fn error(err: ProcedureError) -> Self {
        ProcedureStream::from_future(async move { Err(err) })
    }
}

impl Stream for ProcedureStream {
    type Item = Result<DynOutput, ProcedureError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl std::fmt::Debug for ProcedureStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcedureStream").finish_non_exhaustive()
    }
}

// Internal helper: wraps a Future as a single-item Stream.
// Uses an enum to free the future's memory after completion.
pin_project! {
    #[project = FutureStreamProj]
    enum FutureStream<F> {
        Pending { #[pin] future: F },
        Done,
    }
}

impl<F> Stream for FutureStream<F>
where
    F: Future<Output = Result<DynOutput, ProcedureError>>,
{
    type Item = Result<DynOutput, ProcedureError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.as_mut().project() {
            FutureStreamProj::Pending { future } => {
                let result = std::task::ready!(future.poll(cx));
                self.set(FutureStream::Done);
                Poll::Ready(Some(result))
            }
            FutureStreamProj::Done => Poll::Ready(None),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            FutureStream::Pending { .. } => (1, Some(1)),
            FutureStream::Done => (0, Some(0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn from_future_ok() {
        let stream = ProcedureStream::from_future(async { Ok(DynOutput::new(42u32)) });
        let results: Vec<_> = stream.collect().await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());
        let value = results[0].as_ref().unwrap().to_value().unwrap();
        assert_eq!(value, serde_json::json!(42));
    }

    #[tokio::test]
    async fn from_future_err() {
        let stream = ProcedureStream::from_future(async {
            Err(ProcedureError::Resolver(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found",
            ))))
        });
        let results: Vec<_> = stream.collect().await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }

    #[tokio::test]
    async fn from_future_yields_none_after_first() {
        let mut stream = ProcedureStream::from_future(async { Ok(DynOutput::new("hello")) });
        assert!(stream.next().await.is_some());
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn from_stream_multi_item() {
        let items = vec![
            Ok(DynOutput::new(1u32)),
            Ok(DynOutput::new(2u32)),
            Ok(DynOutput::new(3u32)),
        ];
        let stream = ProcedureStream::from_stream(futures_util::stream::iter(items));
        let results: Vec<_> = stream.collect().await;
        assert_eq!(results.len(), 3);
        for (i, result) in results.iter().enumerate() {
            let value = result.as_ref().unwrap().to_value().unwrap();
            assert_eq!(value, serde_json::json!(i as u32 + 1));
        }
    }

    #[tokio::test]
    async fn from_stream_empty() {
        let stream = ProcedureStream::from_stream(futures_util::stream::empty());
        let results: Vec<Result<DynOutput, ProcedureError>> = stream.collect().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn error_stream() {
        let stream = ProcedureStream::error(ProcedureError::Unwind(Box::new("panic!")));
        let results: Vec<_> = stream.collect().await;
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], Err(ProcedureError::Unwind(_))));
    }

    #[test]
    fn procedure_stream_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ProcedureStream>();
    }

    #[test]
    fn size_hint_from_future() {
        let stream = ProcedureStream::from_future(async { Ok(DynOutput::new(1u32)) });
        let (lower, upper) = stream.size_hint();
        assert_eq!(lower, 1);
        assert_eq!(upper, Some(1));
    }
}
