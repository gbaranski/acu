mod channels;

pub use channels::channel;
pub use channels::Receiver;
pub use channels::Sender;

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(any(test, feature = "master-slave"))] {
        mod master_slave;

        pub use master_slave::MasterMessage;
        pub use master_slave::MasterName;
        pub use master_slave::MasterExt;

        pub use master_slave::MasterHandle;
        pub use master_slave::BroadcasterMasterHandle;
    }
}

pub trait Message: std::fmt::Debug + Send {}

pub trait Name: std::fmt::Debug + std::fmt::Display + Send + Sync + Clone {}

impl Name for &'static str {}

#[cfg(feature = "uuid")]
impl Name for uuid::Uuid {}

#[derive(Debug)]
pub struct Handle<M, N = &'static str>
where
    M: Message,
    N: Name,
{
    pub sender: Sender<M, N>,
}

impl<M, N> Clone for Handle<M, N>
where
    M: Message,
    N: Name,
{
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<M, N> Handle<M, N>
where
    M: Message,
    N: Name,
{
    pub fn name(&self) -> N {
        self.sender.name.clone()
    }
    pub async fn wait_for_stop(&self) {
        self.sender.closed().await;
    }
}

#[cfg(test)]
mod tests {
    use crate::channel;
    use crate::Receiver;
    use crate::Sender;
    use tokio::sync::oneshot;

    #[derive(Debug)]
    enum Message {
        Increment,
        Get { respond_to: oneshot::Sender<usize> },
    }

    impl crate::Message for Message {}

    struct MyActor {
        receiver: Receiver<Message, &'static str>,
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
        sender: Sender<Message, &'static str>,
    }

    impl MyActorHandle {
        pub fn new() -> Self {
            let (sender, receiver) = channel("MyActor");
            let mut actor = MyActor {
                receiver,
                counter: 0,
            };
            tokio::spawn(async move { actor.run().await });
            Self { sender }
        }

        pub async fn increment(&self) {
            self.sender.notify_with(|| Message::Increment).await
        }

        pub async fn get(&self) -> usize {
            self.sender
                .call_with(|respond_to| Message::Get { respond_to })
                .await
        }
    }

    #[tokio::test]
    async fn test_simple() {
        let handle = MyActorHandle::new();
        assert_eq!(handle.get().await, 0);
        for i in 0..100 {
            handle.increment().await;
            assert_eq!(handle.get().await, i + 1);
        }
    }
}
