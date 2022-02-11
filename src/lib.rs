use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub trait Message: std::fmt::Debug + Send + Sync {}

pub trait Name: std::fmt::Debug + std::fmt::Display + Send + Sync + Clone {}

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

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use tokio::sync::oneshot;

    #[derive(Debug, Clone, PartialEq, PartialOrd)]
    enum Name {
        MyActorA,
        MyActorB,
    }

    impl AsRef<str> for Name {
        fn as_ref(&self) -> &str {
            match self {
                Name::MyActorA => "my-actor-a",
                Name::MyActorB => "my-actor-b",
            }
        }
    }

    impl std::fmt::Display for Name {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let s: &str = self.as_ref();
            f.write_str(s)
        }
    }

    impl crate::Name for Name {}

    #[derive(Debug)]
    enum Message {
        Increment,
        Get { respond_to: oneshot::Sender<usize> },
    }

    impl crate::Message for Message {}

    struct MyActor {
        receiver: crate::Receiver<Message, Name>,
        counter: usize,
    }

    impl MyActor {
        async fn run(&mut self) {
            while let Some(message) = self.receiver.recv().await {
                match message {
                    Message::Increment => self.counter += 1,
                    Message::Get { respond_to } => respond_to.send(self.counter).unwrap(),
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    struct MyActorHandle {
        sender: crate::Sender<Message, Name>,
    }

    impl crate::Handle<Message, Name> for MyActorHandle {
        fn sender(&self) -> &crate::Sender<Message, Name> {
            &self.sender
        }
    }

    impl MyActorHandle {
        pub fn new(name: Name) -> Self {
            let (sender, receiver) = crate::channel(8, name);
            let mut actor = MyActor {
                receiver,
                counter: 0,
            };
            tokio::spawn(async move { actor.run().await });
            Self { sender }
        }

        pub async fn increment(&self) {
            self.sender.notify(|| Message::Increment).await
        }

        pub async fn get(&self) -> usize {
            self.sender
                .call(|respond_to| Message::Get { respond_to })
                .await
        }
    }


    #[tokio::test]
    async fn simple() {
        let handle = MyActorHandle::new(Name::MyActorA);
        for _ in 0..100 {
            handle.increment().await;
        }
        assert_eq!(handle.get().await, 100);
    }

    #[tokio::test]
    async fn test_master_and_slave() {
        let handle_a = MyActorHandle::new(Name::MyActorA);
        let handle_b = MyActorHandle::new(Name::MyActorB);
        let master = crate::MasterHandle::new(vec![handle_a.clone(), handle_b.clone()]);
        let get_values = || async {
            let results = master.execute_on_all(|handle| handle.get().boxed()).await;
            assert_eq!(results.len(), 2);
            (results[0], results[1])
        };
        for _ in 0..100 {
            master
                .execute_on_all(|handle| handle.increment().boxed())
                .await;
        }
        assert_eq!(get_values().await, (100, 100));
        {
            let actor_a = master.get(Name::MyActorA).unwrap();
            for _ in 0..10 {
                actor_a.increment().await;
            }
        }
        assert_eq!(get_values().await, (110, 100));

    }
}
