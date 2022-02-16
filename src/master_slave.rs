use crate::channel;
use crate::Handle;
use crate::Message;
use crate::Name;
use crate::Receiver;
use async_trait::async_trait;
use std::marker::PhantomData;
use std::ops::Deref;
use tokio::sync::oneshot;

pub trait MasterName
where
    Self: std::fmt::Debug + std::fmt::Display + Send + Sync + Clone + PartialEq,
{
    fn master_name() -> Self;
}

impl<N: MasterName> Name for N {}

#[derive(Debug)]
pub enum MasterMessage<M, N>
where
    M: Message,
    N: MasterName,
{
    Push {
        handle: Handle<M, N>,
        respond_to: oneshot::Sender<usize>, // returns amount of slaves
    },
    Find {
        name: N,
        respond_to: oneshot::Sender<Option<Handle<M, N>>>,
    },
    Get {
        respond_to: oneshot::Sender<Vec<Handle<M, N>>>,
    },
    __Variant(PhantomData<(N, M)>),
}

impl<M, N> Message for MasterMessage<M, N>
where
    M: Message,
    N: MasterName,
{
}

#[async_trait]
pub trait MasterExt<M, N>
    where
        M: Message,
        N: MasterName,
{
    async fn slaves(&self) -> Vec<Handle<M, N>>;
    async fn find(&self, name: N) -> Option<Handle<M, N>>;
    async fn push(&self, handle: Handle<M, N>) -> usize;
}

#[async_trait]
impl<M, N> MasterExt<M, N> for Handle<MasterMessage<M, N>, N>
    where
        M: Message + 'static,
        N: MasterName + 'static,
{
    async fn slaves(&self) -> Vec<Handle<M, N>> {
        self.sender
            .call_with(|respond_to| MasterMessage::Get { respond_to })
            .await
    }

    async fn find(&self, name: N) -> Option<Handle<M, N>> {
        self.sender
            .call_with(|respond_to| MasterMessage::Find { name, respond_to })
            .await
    }

    async fn push(&self, handle: Handle<M, N>) -> usize {
        self.sender
            .call_with(|respond_to| MasterMessage::Push { handle, respond_to })
            .await
    }
}

// ============================================================================
//
// Master
//
// ============================================================================

#[derive(Debug)]
pub struct Master<M, N>
where
    M: Message,
    N: MasterName,
{
    receiver: Receiver<MasterMessage<M, N>, N>,
    slaves: Vec<Handle<M, N>>,
}

impl<'s, M: Message, N: MasterName> Master<M, N> {
    async fn run(&mut self) {
        while let Some(message) = self.receiver.recv().await {
            self.handle_message(message).await;
        }
    }

    async fn handle_message(&mut self, message: MasterMessage<M, N>) {
        match message {
            MasterMessage::Push { handle, respond_to } => {
                self.slaves.push(handle);
                respond_to.send(self.slaves.len()).unwrap();
            }
            MasterMessage::Get { respond_to } => {
                respond_to.send(self.slaves.clone()).unwrap();
            }
            MasterMessage::Find { name, respond_to } => {
                let slave = self
                    .slaves
                    .iter()
                    .find(|handle| handle.name() == name)
                    .cloned();
                respond_to.send(slave).unwrap();
            }
            MasterMessage::__Variant(_) => unreachable!(),
        }
    }
}

/// Master handle
///
/// Can store slaves inside.
/// If you'd like support for broadcasting messages to slaves, check out `BroadcasterMasterHandle`
pub type MasterHandle<M, N> = Handle<MasterMessage<M, N>, N>;

impl<M, N> MasterHandle<M, N>
where
    M: Message + Send + Sync + 'static,
    N: MasterName + 'static,
{
    pub fn new() -> Self {
        let (sender, receiver) = channel(8, N::master_name());
        let mut actor = Master {
            receiver,
            slaves: vec![],
        };
        let handle = Self { sender };
        tokio::spawn(async move { actor.run().await });
        handle
    }
}

impl<M, N> Default for MasterHandle<M, N>
    where
        M: Message + Send + Sync + 'static,
        N: MasterName + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
//
// Broadcaster master
//
// ============================================================================

#[derive(Debug)]
struct BroadcasterMaster<M, N>
where
    M: Message,
    N: MasterName,
{
    master: Master<M, N>,
    receiver: Receiver<M, N>,
}

impl<'s, M: Message + Clone, N: MasterName> BroadcasterMaster<M, N> {
    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(message) = self.receiver.recv() => {
                    self.handle_message(message).await;
                },
                Some(message) = self.master.receiver.recv() => {
                    self.master.handle_message(message).await;
                },
                else => break
            };
        }
    }

    async fn handle_message(&mut self, message: M) {
        let futures = self.master.slaves.iter().map(|handle| async {
            handle.sender.send(message.clone()).await;
        });
        futures::future::join_all(futures).await;
    }
}

/// BroadcasterMasterHandle
///
/// Can store slaves inside. And supports broadcasting messages to slaves, simply by calling methods on the type.
#[derive(Debug)]
pub struct BroadcasterMasterHandle<M, N>
where
    M: Message,
    N: MasterName,
{
    handle: Handle<M, N>,
    master: Handle<MasterMessage<M, N>, N>,
}

impl<M, N> BroadcasterMasterHandle<M, N>
where
    M: Message + Send + Sync + Clone + 'static,
    N: MasterName + 'static,
{
    pub fn new() -> Self {
        let (sender, receiver) = channel(8, N::master_name());
        let (master_sender, master_receiver) = channel(8, N::master_name());
        let master = Master {
            receiver: master_receiver,
            slaves: vec![],
        };
        let mut broadcaster_master = BroadcasterMaster { master, receiver };
        let handle = Self {
            handle: Handle { sender },
            master: Handle {
                sender: master_sender,
            },
        };
        tokio::spawn(async move { broadcaster_master.run().await });
        handle
    }
}

impl<M, N> Default for BroadcasterMasterHandle<M, N>
    where
        M: Message + Send + Sync + Clone + 'static,
        N: MasterName + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<M, N> Clone for BroadcasterMasterHandle<M, N>
    where
        M: Message,
        N: MasterName,
{
    fn clone(&self) -> Self {
        Self {
            master: self.master.clone(),
            handle: self.handle.clone(),
        }
    }
}

impl<M, N> Deref for BroadcasterMasterHandle<M, N>
where
    M: Message + Send + Sync + Clone + 'static,
    N: MasterName + 'static,
{
    type Target = Handle<M, N>;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

// TODO: Find a way to omit doing this
#[async_trait]
impl<M, N> MasterExt<M, N> for BroadcasterMasterHandle<M, N>
where
    M: Message + Send + Sync + Clone + 'static,
    N: MasterName + 'static,
{
    async fn slaves(&self) -> Vec<Handle<M, N>> {
        self.master.slaves().await
    }

    async fn find(&self, name: N) -> Option<Handle<M, N>> {
        self.master.find(name).await
    }

    async fn push(&self, handle: Handle<M, N>) -> usize {
        self.master.push(handle).await
    }
}

// ============================================================================
//
//
// ============================================================================



#[cfg(test)]
mod tests {
    use crate::channel;
    use crate::BroadcasterMasterHandle;
    use crate::Handle;
    use crate::MasterExt;
    use crate::MasterHandle;
    use tokio::sync::broadcast;

    #[derive(Debug, Clone, PartialEq, PartialOrd)]
    enum Name {
        Master,
        MyActorA,
        MyActorB,
    }

    impl crate::MasterName for Name {
        fn master_name() -> Self {
            Self::Master
        }
    }

    impl AsRef<str> for Name {
        fn as_ref(&self) -> &str {
            match self {
                Name::Master => "master",
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

    #[derive(Debug, Clone)]
    enum Message {
        Increment,
        Fetch {
            respond_to: broadcast::Sender<usize>,
        },
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
                    Message::Fetch { respond_to } => {
                        respond_to.send(self.counter).unwrap();
                    }
                }
            }
        }
    }

    type MyActorHandle = Handle<Message, Name>;

    fn my_actor(name: Name) -> MyActorHandle {
        let (sender, receiver) = channel(8, name);
        let mut actor = MyActor {
            receiver,
            counter: 0,
        };
        tokio::spawn(async move { actor.run().await });
        MyActorHandle { sender }
    }

    use async_trait::async_trait;

    #[async_trait]
    trait MyActorExt {
        async fn increment(&self);
        async fn fetch(&self) -> Vec<usize>;
    }

    #[async_trait]
    impl MyActorExt for Handle<Message, Name> {
        async fn increment(&self) {
            self.sender.notify_with(|| Message::Increment).await
        }

        async fn fetch(&self) -> Vec<usize> {
            self.sender
                .call_many_with(|respond_to| Message::Fetch { respond_to }, 8)
                .await
        }
    }

    #[tokio::test]
    async fn test_broadcast() {
        let handle_a = my_actor(Name::MyActorA);
        let handle_b = my_actor(Name::MyActorB);
        let master = {
            let master = BroadcasterMasterHandle::new();
            master.push(handle_a).await;
            master.push(handle_b).await;
            master
        };
        let get_values = || async {
            let results = master.fetch().await;
            assert_eq!(results.len(), 2);
            (results[0], results[1])
        };
        const BASE: usize = 100;
        for i in 1..=BASE {
            master.increment().await;
            assert_eq!(get_values().await, (i, i));
        }
        {
            let actor_a = master.find(Name::MyActorA).await.unwrap();
            for i in 1..=10 {
                actor_a.increment().await;
                assert_eq!(get_values().await, (BASE + i, BASE));
            }
        }
    }

    #[tokio::test]
    async fn test() {
        let master = {
            let handle_a = my_actor(Name::MyActorA);
            let handle_b = my_actor(Name::MyActorB);
            let master = MasterHandle::new();
            master.push(handle_a).await;
            master.push(handle_b).await;
            master
        };
        let increment = || async {
            master.find(Name::MyActorA).await.unwrap().increment().await;
            master.find(Name::MyActorB).await.unwrap().increment().await;
        };
        let get_values = || async {
            let handle_a = master.find(Name::MyActorA).await.unwrap();
            let handle_b = master.find(Name::MyActorB).await.unwrap();
            (handle_a.fetch().await[0], handle_b.fetch().await[0])
        };
        const BASE: usize = 100;
        for i in 1..=BASE {
            increment().await;
            assert_eq!(get_values().await, (i, i));
        }
        {
            let actor_a = master.find(Name::MyActorA).await.unwrap();
            for i in 1..=10 {
                actor_a.increment().await;
                assert_eq!(get_values().await, (BASE + i, BASE));
            }
        }
    }
}
