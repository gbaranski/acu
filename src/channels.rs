use crate::Message;
use crate::Name;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub struct Sender<M: Message, N: Name = &'static str> {
    pub name: N,
    sender: mpsc::Sender<M>,
}

impl<M: Message, N: Name> Clone for Sender<M, N> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl<M: Message, N: Name> std::fmt::Debug for Sender<M, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender")
            .field("name", &self.name)
            .field(
                "sender",
                if self.sender.is_closed() {
                    &"closed"
                } else {
                    &"open"
                },
            )
            .finish()
    }
}

impl<M: Message, N: Name> Sender<M, N> {
    pub async fn closed(&self) {
        self.sender.closed().await;
    }
}

impl<M: Message, N: Name> Sender<M, N> {
    // TODO: change it to private
    pub async fn send(&self, message: M) {
        #[cfg(feature = "log")]
        log::debug!("send `{:?}` on `{}`", message, self.name);

        self.sender.send(message).await.unwrap();
    }

    /// Send message to the receiver expecting a response.
    /// Message is constructed by calling message_fn
    pub async fn call_with<R: std::fmt::Debug>(
        &self,
        message_fn: impl FnOnce(oneshot::Sender<R>) -> M,
    ) -> R {
        let (sender, receiver) = oneshot::channel();
        let message = message_fn(sender);

        #[cfg(feature = "log")]
        log::debug!("call `{:?}` on `{}`", message, self.name);

        self.sender.send(message).await.unwrap();
        let response = receiver.await.unwrap();

        #[cfg(feature = "log")]
        log::debug!("response `{:?}` from `{}`", response, self.name);

        response
    }

    /// Send message to the receiver expecting a many response.
    /// Message is constructed by calling message_fn
    pub async fn call_many_with<R: std::fmt::Debug + Clone + Send + 'static>(
        &self,
        message_fn: impl FnOnce(broadcast::Sender<R>) -> M,
        capacity: usize,
    ) -> Vec<R> {
        use tokio_stream::wrappers::BroadcastStream;
        use tokio_stream::StreamExt;

        let (sender, receiver) = broadcast::channel(capacity);
        let message = message_fn(sender);

        #[cfg(feature = "log")]
        log::debug!("call many with `{:?}` on `{}`", message, self.name);

        self.sender.send(message).await.unwrap();

        BroadcastStream::new(receiver)
            .filter_map(|res| res.ok())
            .collect()
            .await
    }

    /// Send message to the receiver.
    pub async fn notify(&self, message: M) {
        #[cfg(feature = "log")]
        log::debug!("notify `{:?}` on `{}`", message, self.name);

        self.sender.send(message).await.unwrap();
    }

    /// Send message to the receiver.
    /// Message is constructed by calling message_fn
    pub async fn notify_with(&self, message_fn: impl FnOnce() -> M) {
        let message = message_fn();
        self.notify(message).await;
    }
}

pub struct Receiver<M: Message, N: Name = &'static str> {
    pub name: N,
    receiver: mpsc::Receiver<M>,
}

impl<M: Message, N: Name> std::fmt::Debug for Receiver<M, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Name").field("name", &self.name).finish()
    }
}

impl<M: Message, N: Name> std::ops::Deref for Receiver<M, N> {
    type Target = mpsc::Receiver<M>;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<M: Message, N: Name> std::ops::DerefMut for Receiver<M, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

/// Creates a bounded mpsc channel for communicating between actors with backpressure.
///
/// The channel will buffer up to the provided number of messages.  Once the
/// buffer is full, attempts to send new messages will wait until a message is
/// received from the channel. The provided buffer capacity must be at least 1.
///
/// All data sent on `Sender` will become available on `Receiver` in the same
/// order as it was sent.
///
/// The `Sender` can be cloned to `send` to the same channel from multiple code
/// locations. Only one `Receiver` is supported.
///
/// If the `Receiver` is disconnected while trying to `send`, the `send` method
/// will return a `SendError`. Similarly, if `Sender` is disconnected while
/// trying to `recv`, the `recv` method will return `None`.
///
/// # Panics
///
/// Panics if the buffer capacity is 0.
pub fn channel<M: Message, N: Name>(buffer: usize, name: N) -> (Sender<M, N>, Receiver<M, N>) {
    let (sender, receiver) = mpsc::channel(buffer);
    let sender = Sender {
        name: name.clone(),
        sender,
    };
    let receiver = Receiver { name, receiver };
    (sender, receiver)
}
