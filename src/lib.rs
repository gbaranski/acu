use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub trait Message: std::fmt::Debug + Send + Sync {}

pub trait Name: std::fmt::Debug + std::fmt::Display + AsRef<str> + Send + Sync + Clone {}

impl Name for &'static str {}

pub struct Sender<M: Message, N: Name> {
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
    /// Send message to the receiver expecting a response.
    /// Message is constructed by calling message_fn
    pub async fn call<R: std::fmt::Debug>(
        &self,
        message_fn: impl FnOnce(oneshot::Sender<R>) -> M,
    ) -> R {
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

pub struct Receiver<M: Message, N: Name> {
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

    (
        Sender {
            name: name.clone(),
            sender,
        },
        Receiver {
            name: name.clone(),
            receiver,
        },
    )
}

use async_trait::async_trait;

#[async_trait]
pub trait Handle<M: Message, N: Name>: Send + Sync + 'static {
    fn sender(&self) -> &Sender<M, N>;

    fn name(&self) -> N {
        self.sender().name.clone()
    }

    async fn wait_for_stop(&self) {
        let sender = self.sender().clone();
        sender.closed().await
    }
}

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

pub struct MasterHandle<M: Message, N: Name, H: Handle<M, N>> {
    // TODO: Consider making use of HashSet<H> instead of Vec<H> to optimize `get` function
    slaves: Vec<H>,
    message_phantom: PhantomData<M>,
    name_phantom: PhantomData<N>,
}

impl<M: Message, N: Name, H: Handle<M, N>> MasterHandle<M, N, H> {
    pub fn new(slaves: Vec<H>) -> Self {
        Self {
            slaves,
            message_phantom: Default::default(),
            name_phantom: Default::default(),
        }
    }

    pub fn names(&self) -> Vec<N> {
        self.slaves.iter().map(Handle::name).collect()
    }
}

impl<M: Message, N: Name + PartialOrd, H: Handle<M, N>> MasterHandle<M, N, H> {
    pub fn get(&self, name: N) -> Option<&H> {
        self.slaves.iter().find(|h| h.name() == name)
    }
}

impl<'s, M: Message, N: Name, H: Handle<M, N>> MasterHandle<M, N, H> {
    pub async fn execute_on_all<'a, O>(
        &'s self,
        f: impl Fn(&'s H) -> Pin<Box<dyn Future<Output = O> + Send + 'a>> + 'a,
    ) -> Vec<O> {
        use futures::stream::FuturesOrdered;
        use futures::StreamExt;

        let mut futures = FuturesOrdered::new();
        for future in self.slaves.iter().map(f) {
            futures.push(future)
        }

        futures.collect().await
    }
}
    (Sender { name, sender }, Receiver { name, receiver })
}
