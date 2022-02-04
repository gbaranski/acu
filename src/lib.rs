use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Debug)]
pub struct Sender<M: std::fmt::Debug> {
    name: &'static str,
    sender: mpsc::Sender<M>,
}

impl<M: std::fmt::Debug> Clone for Sender<M> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl<M: std::fmt::Debug> Sender<M> {
    pub async fn closed(&self) {
        self.sender.closed().await;
    }

    /// Send message to the receiver expecting a response.
    /// Message is constructed by calling message_fn
    pub async fn call<R: std::fmt::Debug>(&self, message_fn: impl FnOnce(oneshot::Sender<R>) -> M) -> R {
        let (tx, rx) = oneshot::channel();
        let message = message_fn(tx);

        #[cfg(feature = "log")]
        log::debug!("call `{:?}` on `{}`", message, self.name);

        self.sender.send(message).await.unwrap();
        let response = rx.await.unwrap();

        #[cfg(feature = "log")]
        log::debug!("response `{:?}` from `{}`", response, self.name);

        response
    }

    /// Send message to the receiver.
    /// Message is constructed by calling message_fn
    pub async fn notify(&self, message_fn: impl FnOnce() -> M) {
        let message = message_fn();

        #[cfg(feature = "log")]
        log::debug!("notify `{:?}` on `{}`", message, self.name);

        self.sender.send(message).await.unwrap();
    }
}

#[derive(Debug)]
pub struct Receiver<M: std::fmt::Debug> {
    receiver: mpsc::Receiver<M>,
}

impl<M: std::fmt::Debug> std::ops::Deref for Receiver<M> {
    type Target = mpsc::Receiver<M>;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<M: std::fmt::Debug> std::ops::DerefMut for Receiver<M> {
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
pub fn channel<M: std::fmt::Debug>(buffer: usize, name: &'static str) -> (Sender<M>, Receiver<M>) {
    let (sender, receiver) = mpsc::channel(buffer);

    (Sender { name, sender }, Receiver { receiver })
}
