use futures_core::Stream;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ClientError;
use crate::link::Link;
use crate::rpc_link::RpcLink;

/// oRPC client for calling remote procedures.
///
/// Wraps a [`Link`] implementation and provides typed convenience methods
/// for queries, mutations, and subscriptions.
///
/// # Example
/// ```ignore
/// use orpc_client::Client;
///
/// let client = Client::new("http://localhost:3000/rpc");
///
/// // Query / Mutation
/// let planet: Planet = client.call("planet.find", &FindInput { name: "Earth".into() }).await?;
///
/// // Subscription (SSE stream)
/// use futures_util::StreamExt;
/// let mut stream = client.subscribe::<Planet>("planet.stream", &()).await?;
/// while let Some(result) = stream.next().await {
///     let planet = result?;
///     println!("New planet: {planet:?}");
/// }
/// ```
pub struct Client<L = RpcLink> {
    link: L,
}

impl Client<RpcLink> {
    /// Create a new client with a default [`RpcLink`] targeting the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Client {
            link: RpcLink::new(base_url),
        }
    }
}

impl<L: Link> Client<L> {
    /// Create a client with a custom [`Link`] implementation.
    pub fn with_link(link: L) -> Self {
        Client { link }
    }

    /// Call a procedure and deserialize the response.
    ///
    /// Works for both queries (read) and mutations (write).
    pub async fn call<I, O>(&self, path: &str, input: &I) -> Result<O, ClientError>
    where
        I: Serialize,
        O: DeserializeOwned,
    {
        let input_value = serde_json::to_value(input).map_err(ClientError::Serialize)?;
        let output_value = self.link.call(path, input_value).await?;
        serde_json::from_value(output_value).map_err(ClientError::Deserialize)
    }

    /// Subscribe to a streaming procedure.
    ///
    /// Returns a stream of deserialized values. The stream ends when the
    /// server sends a `done` event or an `error` event.
    pub async fn subscribe<O>(
        &self,
        path: &str,
        input: &impl Serialize,
    ) -> Result<impl Stream<Item = Result<O, ClientError>>, ClientError>
    where
        O: DeserializeOwned + 'static,
    {
        self.subscribe_from(path, input, None).await
    }

    /// Subscribe with a specific `last_event_id` for SSE reconnection.
    ///
    /// The server will resume from events after `last_event_id`.
    pub async fn subscribe_from<O>(
        &self,
        path: &str,
        input: &impl Serialize,
        last_event_id: Option<u64>,
    ) -> Result<impl Stream<Item = Result<O, ClientError>>, ClientError>
    where
        O: DeserializeOwned + 'static,
    {
        let input_value = serde_json::to_value(input).map_err(ClientError::Serialize)?;
        let value_stream = self
            .link
            .subscribe(path, input_value, last_event_id)
            .await?;

        Ok(DeserializeStream {
            inner: value_stream,
            _phantom: std::marker::PhantomData,
        })
    }
}

use std::pin::Pin;
use std::task::{Context, Poll};

pin_project_lite::pin_project! {
    struct DeserializeStream<O> {
        #[pin]
        inner: Pin<Box<dyn Stream<Item = Result<serde_json::Value, ClientError>> + Send>>,
        _phantom: std::marker::PhantomData<O>,
    }
}

impl<O: DeserializeOwned> Stream for DeserializeStream<O> {
    type Item = Result<O, ClientError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        match this.inner.poll_next(cx) {
            Poll::Ready(Some(Ok(value))) => {
                let result = serde_json::from_value(value).map_err(ClientError::Deserialize);
                Poll::Ready(Some(result))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
