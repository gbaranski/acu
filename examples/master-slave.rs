use acu::MasterExt;
use acu::MasterHandle;
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Name {
    Master,
    MyActorA,
    MyActorB,
}

impl acu::MasterName for Name {
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

#[derive(Debug)]
enum Message {
    Increment,
    Fetch { respond_to: oneshot::Sender<usize> },
}

impl acu::Message for Message {}

struct MyActor {
    receiver: acu::Receiver<Message, Name>,
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

fn my_actor(name: Name) -> MyActorHandle {
    let (sender, receiver) = acu::channel(name);
    let mut actor = MyActor {
        receiver,
        counter: 0,
    };
    tokio::spawn(async move { actor.run().await });
    MyActorHandle { sender }
}

type MyActorHandle = acu::Handle<Message, Name>;

use async_trait::async_trait;

#[async_trait]
trait MyActorExt {
    async fn increment(&self);
    async fn fetch(&self) -> usize;
}

#[async_trait]
impl MyActorExt for MyActorHandle {
    async fn increment(&self) {
        self.sender.notify_with(|| Message::Increment).await
    }

    async fn fetch(&self) -> usize {
        self.sender
            .call_with(|respond_to| Message::Fetch { respond_to })
            .await
    }
}

#[tokio::main]
async fn main() {
    let handle_a = my_actor(Name::MyActorA);
    let handle_b = my_actor(Name::MyActorB);
    let master = {
        let master = MasterHandle::new();
        master.push(handle_a).await;
        master.push(handle_b).await;
        master
    };
    let get_handles = || async {
        let handle_a = master.find(Name::MyActorA).await.unwrap();
        let handle_b = master.find(Name::MyActorA).await.unwrap();
        (handle_a, handle_b)
    };
    let get_values = || async {
        let (handle_a, handle_b) = get_handles().await;
        (handle_a.fetch().await, handle_b.fetch().await)
    };
    let print_values = || async {
        let values = get_values().await;
        println!("counter of MyActorA = {}", values.0);
        println!("counter of MyActorB = {}", values.1);
        println!();
    };
    for _ in 0..100 {
        let (handle_a, handle_b) = get_handles().await;
        handle_a.increment().await;
        handle_b.increment().await;
        print_values().await;
    }
    print_values().await;
    {
        let actor_a = master.find(Name::MyActorA).await.unwrap();
        for _ in 0..10 {
            actor_a.increment().await;
        }
    }
    print_values().await;
}
